## Log locations
For the EC2 instance, accessing `/var` requires root user or `sudo`
### `/var/log/kafka`
This contains plain text log files split by hour.
``` [ controller | server | connect ].log.yyyy-mm-dd-hh ```
> `connect.log.2026-07-03-12` or the connect log from July 3 2026 at 12 PM

### `/var/lib/kafka`
This contains directories with binary log files that need to be translated with
A typical log directory here will contain:
* `.log`
* `.timestamp`
* `.index`