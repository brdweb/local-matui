# Matui development

Matui is a terminal user interface for Music Assistant. This repository is the
source of truth for the application.

## Initial state

No application code, language, framework, build commands, or tests exist yet.
Confirm the initial scope and choose the stack before implementation. Consult
current official Music Assistant API/client documentation when designing the
integration; do not assume compatibility with an unverified server version.

## Working conventions

- Keep work scoped to this repository. Preserve unrelated changes.
- Record architecture decisions and actual setup/test commands as they emerge.
- Keep credentials, server tokens, local configuration, and personal media data
  out of Git. Provide sanitized examples when configuration is introduced.
- Test relevant behavior before proposing a merge; do not claim checks passed
  unless they ran.
- Use feature branches and reviewable pull requests for application changes.
- Obtain explicit authorization before changing a live Music Assistant server,
  controlling playback, publishing releases, or deploying services.
