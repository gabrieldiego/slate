SLATE_RUST_TOOLCHAIN ?= 1.97.1
SLATE_RUST_COMPONENTS ?= rustfmt clippy
SLATE_LOCAL_RUSTUP_HOME ?= $(CURDIR)/.rustup
SLATE_LOCAL_CARGO_HOME ?= $(CURDIR)/.cargo
HOST_CARGO := $(shell command -v cargo 2>/dev/null)
HOST_RUSTUP := $(shell command -v rustup 2>/dev/null)
CARGO_BIN ?= $(if $(HOST_CARGO),$(HOST_CARGO),cargo)
RUSTUP_BIN ?= $(if $(HOST_RUSTUP),$(HOST_RUSTUP),rustup)
SLATE_LOCAL_RUST_ENV := RUSTUP_HOME="$(SLATE_LOCAL_RUSTUP_HOME)" CARGO_HOME="$(SLATE_LOCAL_CARGO_HOME)"
SLATE_LOCAL_RUSTUP_ENV := RUSTUP_HOME="$(SLATE_LOCAL_RUSTUP_HOME)"
SLATE_CARGO := $(SLATE_LOCAL_RUST_ENV) "$(CARGO_BIN)"
SLATE_RUSTUP := $(SLATE_LOCAL_RUSTUP_ENV) "$(RUSTUP_BIN)"

SLATE_LAUNCH_TARGET_DIR ?= target/slate-launch
SLATE_SHARED_TARGET_DIR ?= target/slate-shared
CARGO_BUILD_JOBS ?= 1
SLATE_SHARED_COMMON_RUSTFLAGS ?= -C prefer-dynamic -C rpath
SLATE_SHARED_DEBUG_RUSTFLAGS ?= $(SLATE_SHARED_COMMON_RUSTFLAGS) -C link-arg=-Wl,-rpath,$$ORIGIN/$(SLATE_SHARED_TARGET_DIR)/debug -C link-arg=-Wl,-rpath,$$ORIGIN/$(SLATE_SHARED_TARGET_DIR)/debug/deps
SLATE_SHARED_RELEASE_RUSTFLAGS ?= $(SLATE_SHARED_COMMON_RUSTFLAGS) -C link-arg=-Wl,-rpath,$$ORIGIN/$(SLATE_SHARED_TARGET_DIR)/release -C link-arg=-Wl,-rpath,$$ORIGIN/$(SLATE_SHARED_TARGET_DIR)/release/deps

ROOT_SLATE_BIN := slate
ROOT_SLATE_BIN_TMP := $(ROOT_SLATE_BIN).tmp
SLATE_LAUNCH_DEBUG_BIN := $(SLATE_LAUNCH_TARGET_DIR)/debug/slate
SLATE_LAUNCH_RELEASE_BIN := $(SLATE_LAUNCH_TARGET_DIR)/release/slate
SLATE_SHARED_DEBUG_BIN := $(SLATE_SHARED_TARGET_DIR)/debug/slate
SLATE_SHARED_RELEASE_BIN := $(SLATE_SHARED_TARGET_DIR)/release/slate
SLATE_SHARED_DEBUG_LIB_PATH := $(SLATE_SHARED_TARGET_DIR)/debug/deps:$(SLATE_SHARED_TARGET_DIR)/debug
SLATE_SHARED_RELEASE_LIB_PATH := $(SLATE_SHARED_TARGET_DIR)/release/deps:$(SLATE_SHARED_TARGET_DIR)/release

.PHONY: check-tools setup-local-rust ensure-local-rust slate-bin slate-release-bin slate-shared-bin slate-shared-release-bin release shared share shared-release share-release run run-release run-shared run-share run-shared-release run-share-release test-broadwebd test-network test-external-network clean-slate-bin clean-object-data

check-tools:
	@set -eu; \
	missing=0; \
	for tool in "$(RUSTUP_BIN)" "$(CARGO_BIN)" cc c++ pkg-config; do \
		if [ -x "$$tool" ] || command -v "$$tool" >/dev/null 2>&1; then \
			printf 'found %s\n' "$$tool"; \
		else \
			printf 'missing %s\n' "$$tool"; \
			missing=1; \
		fi; \
	done; \
	if $(SLATE_RUSTUP) run "$(SLATE_RUST_TOOLCHAIN)" rustc --version >/dev/null 2>&1; then \
		printf 'local Rust toolchain %s is installed\n' "$(SLATE_RUST_TOOLCHAIN)"; \
	else \
		printf 'local Rust toolchain %s is missing; run: make setup-local-rust\n' "$(SLATE_RUST_TOOLCHAIN)"; \
		missing=1; \
	fi; \
	exit "$$missing"

setup-local-rust:
	@set -eu; \
	mkdir -p "$(SLATE_LOCAL_RUSTUP_HOME)" "$(SLATE_LOCAL_CARGO_HOME)"; \
	$(SLATE_RUSTUP) toolchain install "$(SLATE_RUST_TOOLCHAIN)" --profile minimal $(addprefix --component ,$(SLATE_RUST_COMPONENTS)); \
	$(SLATE_RUSTUP) run "$(SLATE_RUST_TOOLCHAIN)" rustc --version; \
	$(SLATE_RUSTUP) run "$(SLATE_RUST_TOOLCHAIN)" cargo --version

ensure-local-rust:
	@$(SLATE_RUSTUP) run "$(SLATE_RUST_TOOLCHAIN)" rustc --version >/dev/null 2>&1 || { \
		printf 'Local Rust toolchain %s is missing. Run: make setup-local-rust\n' "$(SLATE_RUST_TOOLCHAIN)"; \
		exit 1; \
	}

slate-bin: ensure-local-rust
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" $(SLATE_CARGO) build -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_LAUNCH_DEBUG_BIN)" "$(ROOT_SLATE_BIN_TMP)"
	chmod +x "$(ROOT_SLATE_BIN_TMP)"
	mv -f "$(ROOT_SLATE_BIN_TMP)" "$(ROOT_SLATE_BIN)"

slate-release-bin: ensure-local-rust
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" $(SLATE_CARGO) build --release -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_LAUNCH_RELEASE_BIN)" "$(ROOT_SLATE_BIN_TMP)"
	chmod +x "$(ROOT_SLATE_BIN_TMP)"
	mv -f "$(ROOT_SLATE_BIN_TMP)" "$(ROOT_SLATE_BIN)"

slate-shared-bin: ensure-local-rust
	RUSTFLAGS='$(SLATE_SHARED_DEBUG_RUSTFLAGS)' CARGO_TARGET_DIR="$(SLATE_SHARED_TARGET_DIR)" $(SLATE_CARGO) build -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_SHARED_DEBUG_BIN)" "$(ROOT_SLATE_BIN_TMP)"
	chmod +x "$(ROOT_SLATE_BIN_TMP)"
	mv -f "$(ROOT_SLATE_BIN_TMP)" "$(ROOT_SLATE_BIN)"

slate-shared-release-bin: ensure-local-rust
	RUSTFLAGS='$(SLATE_SHARED_RELEASE_RUSTFLAGS)' CARGO_TARGET_DIR="$(SLATE_SHARED_TARGET_DIR)" $(SLATE_CARGO) build --release -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_SHARED_RELEASE_BIN)" "$(ROOT_SLATE_BIN_TMP)"
	chmod +x "$(ROOT_SLATE_BIN_TMP)"
	mv -f "$(ROOT_SLATE_BIN_TMP)" "$(ROOT_SLATE_BIN)"

release: slate-release-bin

shared: slate-shared-bin

share: shared

shared-release: slate-shared-release-bin

share-release: shared-release

run: slate-bin
	./$(ROOT_SLATE_BIN) $(ARGS)

run-release: slate-release-bin
	./$(ROOT_SLATE_BIN) $(ARGS)

run-shared: slate-shared-bin
	LD_LIBRARY_PATH="$(SLATE_SHARED_DEBUG_LIB_PATH):$$LD_LIBRARY_PATH" "./$(ROOT_SLATE_BIN)" $(ARGS)

run-share: run-shared

run-shared-release: slate-shared-release-bin
	LD_LIBRARY_PATH="$(SLATE_SHARED_RELEASE_LIB_PATH):$$LD_LIBRARY_PATH" "./$(ROOT_SLATE_BIN)" $(ARGS)

run-share-release: run-shared-release

test-broadwebd: ensure-local-rust
	$(SLATE_CARGO) test -j "$(CARGO_BUILD_JOBS)" -p slate-broadwebd

test-network: ensure-local-rust
	$(SLATE_CARGO) test -j "$(CARGO_BUILD_JOBS)" -p slate-broadwebd
	CARGO_BUILD_JOBS="$(CARGO_BUILD_JOBS)" $(SLATE_CARGO) test -p slate-browser-core navigating_http_hands_page_to_servo

test-external-network: ensure-local-rust
	SLATE_EXTERNAL_NETWORK_TESTS=1 $(SLATE_CARGO) test -j "$(CARGO_BUILD_JOBS)" -p slate-broadwebd external_ -- --ignored

clean-slate-bin:
	rm -f "$(ROOT_SLATE_BIN)"

clean-object-data:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" $(SLATE_CARGO) clean
	CARGO_TARGET_DIR="$(SLATE_SHARED_TARGET_DIR)" $(SLATE_CARGO) clean
	$(SLATE_CARGO) clean
	rm -f "$(ROOT_SLATE_BIN)"
