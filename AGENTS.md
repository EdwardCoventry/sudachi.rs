# Agent instructions

## Local verification and GitHub usage

Run routine checks locally before pushing. Follow [.github/LOCAL_CI.md](.github/LOCAL_CI.md)
and `pwsh -NoProfile -File .github/local-checks.ps1`. GitHub runs are manual-only:
do not dispatch or rerun them unless the user explicitly requests hosted execution.
Preserve release and publishing commands; a push or tag must not launch cloud CI.
