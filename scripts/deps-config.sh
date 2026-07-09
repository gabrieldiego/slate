# Shared dependency defaults for local builds and CI.
#
# Keep the upstream NetSurf base for dependencies we have not forked. When Slate
# forks a dependency, add a per-project *_REPO_URI override here instead of
# changing REPO_BASE_URI globally.
export REPO_BASE_URI="${REPO_BASE_URI:-https://github.com/netsurf-browser}"

export QUICKJS_REPO_URI="${QUICKJS_REPO_URI:-https://github.com/bellard/quickjs}"
export QUICKJS_REF="${QUICKJS_REF:-04be246001599f5995fa2f2d8c91a0f198d3f34c}"
