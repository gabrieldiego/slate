SLATE_LAUNCH_TARGET_DIR ?= target/slate-launch
CARGO_BUILD_JOBS ?= 1

ROOT_SLATE_BIN := slate
SLATE_LAUNCH_DEBUG_BIN := $(SLATE_LAUNCH_TARGET_DIR)/debug/slate
SLATE_LAUNCH_RELEASE_BIN := $(SLATE_LAUNCH_TARGET_DIR)/release/slate

.PHONY: slate-bin slate-release-bin release run run-release clean-slate-bin clean-object-data

slate-bin:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo build -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_LAUNCH_DEBUG_BIN)" "$(ROOT_SLATE_BIN)"
	chmod +x "$(ROOT_SLATE_BIN)"

slate-release-bin:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo build --release -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_LAUNCH_RELEASE_BIN)" "$(ROOT_SLATE_BIN)"
	chmod +x "$(ROOT_SLATE_BIN)"

release: slate-release-bin

run: slate-bin
	./$(ROOT_SLATE_BIN) $(ARGS)

run-release: slate-release-bin
	./$(ROOT_SLATE_BIN) $(ARGS)

clean-slate-bin:
	rm -f "$(ROOT_SLATE_BIN)"

clean-object-data:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo clean
	cargo clean
	rm -f "$(ROOT_SLATE_BIN)"
