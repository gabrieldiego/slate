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

.PHONY: slate-bin slate-release-bin slate-shared-bin slate-shared-release-bin release shared share shared-release share-release run run-release run-shared run-share run-shared-release run-share-release test-broadwebd test-network test-external-network clean-slate-bin clean-object-data

slate-bin:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo build -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_LAUNCH_DEBUG_BIN)" "$(ROOT_SLATE_BIN_TMP)"
	chmod +x "$(ROOT_SLATE_BIN_TMP)"
	mv -f "$(ROOT_SLATE_BIN_TMP)" "$(ROOT_SLATE_BIN)"

slate-release-bin:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo build --release -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_LAUNCH_RELEASE_BIN)" "$(ROOT_SLATE_BIN_TMP)"
	chmod +x "$(ROOT_SLATE_BIN_TMP)"
	mv -f "$(ROOT_SLATE_BIN_TMP)" "$(ROOT_SLATE_BIN)"

slate-shared-bin:
	RUSTFLAGS='$(SLATE_SHARED_DEBUG_RUSTFLAGS)' CARGO_TARGET_DIR="$(SLATE_SHARED_TARGET_DIR)" cargo build -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_SHARED_DEBUG_BIN)" "$(ROOT_SLATE_BIN_TMP)"
	chmod +x "$(ROOT_SLATE_BIN_TMP)"
	mv -f "$(ROOT_SLATE_BIN_TMP)" "$(ROOT_SLATE_BIN)"

slate-shared-release-bin:
	RUSTFLAGS='$(SLATE_SHARED_RELEASE_RUSTFLAGS)' CARGO_TARGET_DIR="$(SLATE_SHARED_TARGET_DIR)" cargo build --release -j "$(CARGO_BUILD_JOBS)" -p slate
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

test-broadwebd:
	cargo test -j "$(CARGO_BUILD_JOBS)" -p slate-broadwebd

test-network:
	cargo test -j "$(CARGO_BUILD_JOBS)" -p slate-broadwebd
	CARGO_BUILD_JOBS="$(CARGO_BUILD_JOBS)" cargo test -p slate-browser-core navigating_http_hands_page_to_servo

test-external-network:
	SLATE_EXTERNAL_NETWORK_TESTS=1 cargo test -j "$(CARGO_BUILD_JOBS)" -p slate-broadwebd external_ -- --ignored

clean-slate-bin:
	rm -f "$(ROOT_SLATE_BIN)"

clean-object-data:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo clean
	CARGO_TARGET_DIR="$(SLATE_SHARED_TARGET_DIR)" cargo clean
	cargo clean
	rm -f "$(ROOT_SLATE_BIN)"
