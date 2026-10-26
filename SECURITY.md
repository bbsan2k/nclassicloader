# Security

Classic Menu is experimental software. There are no supported production releases
or guarantees of data recovery. Keep original images and independent save backups.

## Reporting a vulnerability

Use GitHub's **Security → Advisories → Report a vulnerability** when private
reporting is enabled. If it is unavailable, open an issue asking for a private
reporting channel, without exploit details, secrets or copyrighted content.
Do not attach console images, ROMs, firmware, saves, pairing codes or browser
session credentials. A minimal, self-authored reproduction is preferred.

Please include the affected revision, platform, impact, reproduction and whether
the issue involves importing files, native core access or local network transfer.
Do not test against devices or networks you do not own or have permission to use.

## Trust boundaries and limitations

- Imported images are untrusted data. The importer must never execute embedded
  programs or scripts. Parsing and emulation are not security sandboxes.
- iOS imports through Files and does not start a LAN listener.
- tvOS browser transfer is foreground-only and intended for a **trusted LAN**.
  It uses unencrypted HTTP. Pairing is not protection against a network observer;
  do not expose the listener to the internet or port-forward it.
- Six-digit PINs expire after ten minutes and are exchanged for independent
  browser credentials. Five wrong attempts lock pairing globally for a minute.
  Credential refresh and backgrounding revoke existing browser sessions.
- tvOS caches, including saves, can be evicted. Power loss, force termination,
  external file-provider behavior and physical-device recovery are not certified.

Repository-content checks are deliberately limited heuristics, not a complete
secret scanner, vulnerability assessment or legal review. Review staged changes
and dependency notices before publishing.
