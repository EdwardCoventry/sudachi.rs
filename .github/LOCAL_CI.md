# Local verification and explicit hosted runs

Run routine verification on the local computer before committing or pushing:

```powershell
pwsh -NoProfile -File .github/local-checks.ps1
pwsh -NoProfile -File .github/local-checks.ps1 -List
```

Available tasks: `check`, `release-check`, `python`. Select another task with `-Task <name>`.
The runner uses the repository's existing checks, keeps independent failures visible,
and records command exits and logs under ignored `.github/local-output/`.
Install missing dependencies in the project's existing environment; an install
failure is an error, not a reason to skip a check.

GitHub workflow runs require an explicit owner or cloud-agent request. Pushes,
pull requests, tag creation, and recurring timers must not launch hosted jobs.
Local agents run the commands above and must not dispatch or rerun GitHub jobs
unless the user explicitly requests a hosted run.

For an explicitly requested hosted task, choose its branch or tag and dispatch it:

```powershell
gh workflow run <workflow.yml> --ref <branch-or-tag>
```

Release workflows that publish packages require `-f release_tag=<existing-tag>`.
Tag pushes no longer publish automatically. Existing authenticated local publishing
commands remain available. npm trusted publishing keeps its unchanged GitHub
workflow filename and OIDC permissions; request that manual release when OIDC is needed.

Cross-platform checks and release artifacts require the corresponding local
platform/toolchain, or an explicitly requested hosted matrix run. A local
Windows result does not certify macOS, Linux, Android, deployed services, or production.

The `python` task builds the native bindings with the installed Rust toolchain and tests them in an isolated `.github/local-output/python-bindings-env` virtual environment. It supports Windows and POSIX Python without changing the active project environment.
