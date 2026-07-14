# Arrow Navigation & Directory Download — Design Overview

## User Flow: Directory Download

```mermaid
flowchart TD
    A[User presses 'c' on selected entry] --> B{Entry type?}
    B -->|File/Symlink| C[Prompt for local path → download file]
    B -->|Directory| D[Probe directory size via du -sb]
    D -->|Error| E[Show error, stay in browser]
    D -->|Success| F[Show confirmation: path + size]
    F -->|y| G[Prompt for local destination]
    F -->|n / Esc| H[Return to browser]
    G -->|Confirm| I{Destination exists?}
    I -->|Yes| J[Confirm overwrite → delete existing → download]
    I -->|No| K[Create directory → download]
    J --> L[Recursive download]
    K --> L
    L --> M[Show result: bytes transferred + any failures]
```

## User Flow: Arrow Navigation

```mermaid
flowchart LR
    Left["← Left Arrow"] --> GoUp["Go to parent directory"]
    Right["→ Right Arrow"] --> Enter["Enter selected directory"]
    Up["↑ Up Arrow"] --> CursorUp["Move cursor up"]
    Down["↓ Down Arrow"] --> CursorDown["Move cursor down"]
```

Left/Right are alternative bindings for Backspace and Enter respectively.

## Module Architecture

```mermaid
graph TD
    subgraph UI["ui/"]
        mod_ui["mod.rs — render dispatch"]
        browser["browser.rs — file list widget"]
        path_prompt["path_prompt.rs — copy prompts + dir confirm"]
        header["header.rs"]
        footer["footer.rs"]
    end

    subgraph Operations["operations/"]
        download["download.rs — file & directory copy"]
        listing["listing.rs — dir listing, sorting, paths"]
        search["search.rs — local & global find"]
        navigate["navigate.rs — direct path nav"]
    end

    subgraph Core["core"]
        app["app.rs — state machine (AppMode, App)"]
        event["event.rs — key → Action mapping"]
        main["main.rs — event loop + dispatch"]
        types["types.rs — DirectoryEntry, StatusMessage"]
        ssh["ssh.rs — SshClient (SFTP + sudo fallback via exec)"]
    end

    main --> event
    main --> app
    main --> Operations
    main --> mod_ui
    event --> app
    mod_ui --> app
    mod_ui --> browser
    mod_ui --> path_prompt
    Operations --> ssh
    Operations --> types
```

## State Machine (AppMode)

```mermaid
stateDiagram-v2
    [*] --> Browsing
    Browsing --> Browsing : Arrow keys, cursor movement
    Browsing --> Browsing : Enter directory / Go parent
    Browsing --> PathPrompt : 'c' on file
    Browsing --> DirectoryCopyConfirm : 'c' on directory
    Browsing --> SearchPrompt : Ctrl+F / Ctrl+G
    Browsing --> NavigatePrompt : 'm'

    DirectoryCopyConfirm --> PathPrompt : 'y'
    DirectoryCopyConfirm --> Browsing : 'n' / Esc

    PathPrompt --> OverwriteConfirm : destination exists
    PathPrompt --> Browsing : confirm (download completes)
    PathPrompt --> Browsing : cancel / error

    OverwriteConfirm --> Browsing : 'y' (overwrite + download)
    OverwriteConfirm --> Browsing : 'n' / Esc

    SearchPrompt --> Searching : confirm query
    SearchPrompt --> Browsing : cancel
    Searching --> SearchResults : results ready
    SearchResults --> Browsing : select result / cancel
    NavigatePrompt --> Browsing : confirm / cancel
```

## Sudo Fallback for Privileged Directories

When SFTP operations fail with `PermissionDenied`, the SSH client transparently escalates to `sudo` via exec channels:

```mermaid
flowchart TD
    A[SFTP operation] -->|PermissionDenied| B{Operation type?}
    A -->|Success| C[Return result]
    A -->|Other error| D[Return error]
    B -->|list_dir| E["sudo ls -la (exec channel)"]
    B -->|download_file| F["sudo cat (exec channel)"]
    E -->|stdout| G[parse_ls_la_output → Vec of DirectoryEntry]
    F -->|stdout| H[Stream 8 KiB chunks to writer]
    E -->|stderr contains 'sudo:' / 'password is required'| I[Return PermissionDenied]
    F -->|stderr contains 'sudo:' / 'password is required'| I
```

Key implementation details:

- **`sudo_ls_la`** — Runs `LC_ALL=C sudo ls -la <path>` over an exec channel. Output is parsed field-by-field (permissions → type, field 4 → size, fields 8+ → filename). Symlink targets (` -> ...`) are stripped from names. `.` and `..` are excluded.
- **`sudo_cat`** — Runs `sudo cat <path>` over an exec channel. Streams stdout to the caller's writer in 8 KiB chunks and returns total bytes written.
- **Error logging** — On `PermissionDenied` (before fallback), a timestamped line is appended to `rfv-errors.log` in the working directory. Logging is best-effort and failures are silently ignored.
- **Passwordless sudo required** — If stderr indicates a password prompt, the fallback returns `PermissionDenied` rather than hanging. The feature assumes NOPASSWD sudo is configured on the remote host.

## Key Design Decisions

- **No progress mode**: The recursive download blocks the event loop. The UI freezes during download. This matches the existing behavior for all SSH operations (file copy, search, listing).
- **Entry-type branching at dispatch time**: The PathPrompt and OverwriteConfirm handlers check `app.selected_entry().entry_type` to decide file vs directory copy logic — no extra state needed.
- **Reused actions**: `DirectoryCopyConfirm` uses existing `ConfirmOverwrite`/`DenyOverwrite` actions. The dispatcher differentiates by checking the current `AppMode`.
- **Best-effort downloads**: Individual file failures are recorded and skipped. The user sees a summary at the end.
- **Transparent escalation**: Sudo fallback is invisible to callers of `list_dir` and `download_file` — the same `Result` types are returned regardless of which path was taken.
