export PS1='$ '
: "${HEADLESS_SHELL:?Set HEADLESS_SHELL to a dedicated headless-shell executable}"
export HEADLESS_REAPER_PATH="$(mktemp -d /tmp/reaper-demo.XXXXXX)"
export HEADLESS_REAPER_GRACE=1s
export PATH="$PWD/rust/target/release:$PATH"
clear
