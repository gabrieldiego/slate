SLATE_LAUNCH_TARGET_DIR ?= target/slate-launch
SLATE_SHARED_TARGET_DIR ?= target/slate-shared
CARGO_BUILD_JOBS ?= 1
SLATE_SHARED_RUSTFLAGS ?= -C prefer-dynamic -C rpath

ROOT_SLATE_BIN := slate
SLATE_LAUNCH_DEBUG_BIN := $(SLATE_LAUNCH_TARGET_DIR)/debug/slate
SLATE_LAUNCH_RELEASE_BIN := $(SLATE_LAUNCH_TARGET_DIR)/release/slate
SLATE_SHARED_DEBUG_BIN := $(SLATE_SHARED_TARGET_DIR)/debug/slate
SLATE_SHARED_RELEASE_BIN := $(SLATE_SHARED_TARGET_DIR)/release/slate
SLATE_SHARED_DEBUG_LIB_PATH := $(SLATE_SHARED_TARGET_DIR)/debug/deps:$(SLATE_SHARED_TARGET_DIR)/debug
SLATE_SHARED_RELEASE_LIB_PATH := $(SLATE_SHARED_TARGET_DIR)/release/deps:$(SLATE_SHARED_TARGET_DIR)/release

.PHONY: slate-bin slate-release-bin slate-shared-bin slate-shared-release-bin release shared shared-release run run-release run-shared run-shared-release clean-slate-bin clean-object-data

slate-bin:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo build -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_LAUNCH_DEBUG_BIN)" "$(ROOT_SLATE_BIN)"
	chmod +x "$(ROOT_SLATE_BIN)"

slate-release-bin:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo build --release -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_LAUNCH_RELEASE_BIN)" "$(ROOT_SLATE_BIN)"
	chmod +x "$(ROOT_SLATE_BIN)"

slate-shared-bin:
	RUSTFLAGS="$(SLATE_SHARED_RUSTFLAGS)" CARGO_TARGET_DIR="$(SLATE_SHARED_TARGET_DIR)" cargo build -j "$(CARGO_BUILD_JOBS)" -p slate

slate-shared-release-bin:
	RUSTFLAGS="$(SLATE_SHARED_RUSTFLAGS)" CARGO_TARGET_DIR="$(SLATE_SHARED_TARGET_DIR)" cargo build --release -j "$(CARGO_BUILD_JOBS)" -p slate

release: slate-release-bin

shared: slate-shared-bin

shared-release: slate-shared-release-bin

run: slate-bin
	./$(ROOT_SLATE_BIN) $(ARGS)

run-release: slate-release-bin
	./$(ROOT_SLATE_BIN) $(ARGS)

run-shared: slate-shared-bin
	LD_LIBRARY_PATH="$(SLATE_SHARED_DEBUG_LIB_PATH):$$LD_LIBRARY_PATH" "$(SLATE_SHARED_DEBUG_BIN)" $(ARGS)

run-shared-release: slate-shared-release-bin
	LD_LIBRARY_PATH="$(SLATE_SHARED_RELEASE_LIB_PATH):$$LD_LIBRARY_PATH" "$(SLATE_SHARED_RELEASE_BIN)" $(ARGS)

clean-slate-bin:
	rm -f "$(ROOT_SLATE_BIN)"

clean-object-data:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo clean
	CARGO_TARGET_DIR="$(SLATE_SHARED_TARGET_DIR)" cargo clean
	cargo clean
	rm -f "$(ROOT_SLATE_BIN)"
