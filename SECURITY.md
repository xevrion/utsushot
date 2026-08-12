# Security Policy

## Supported versions

utsushot is pre-1.0. Only the latest release receives fixes.

## Reporting a vulnerability

Please report privately rather than opening a public issue.

Use GitHub's private reporting: go to the [Security
tab](https://github.com/xevrion/utsushot/security/advisories/new) and open a
draft advisory. This is the preferred route and reaches the maintainers
directly.

Include what you were running, what happened, and how to reproduce it. You can
expect an initial response within a week.

## Scope

utsushot manipulates compositor output configuration and writes image files, so
the things worth reporting are roughly:

- A capture that leaves the session in a broken or unrecoverable state
- Screenshots written to a path other than the one requested, or with
  permissions that expose them to other users
- Capturing content the user did not consent to, such as another session's
  output
- Command injection through output names, file paths, or other values passed to
  `grim`, `wl-copy`, or `notify-send`

Bugs where a capture simply fails cleanly are ordinary issues, not security
reports.
