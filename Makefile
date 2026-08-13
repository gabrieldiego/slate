SLATE_LAUNCH_TARGET_DIR ?= target/slate-launch
CARGO_BUILD_JOBS ?= 1

ROOT_SLATE_BIN := slate
SLATE_LAUNCH_BIN := $(SLATE_LAUNCH_TARGET_DIR)/debug/slate

.PHONY: slate-bin run clean-slate-bin

slate-bin:
	CARGO_TARGET_DIR="$(SLATE_LAUNCH_TARGET_DIR)" cargo build -j "$(CARGO_BUILD_JOBS)" -p slate
	cp "$(SLATE_LAUNCH_BIN)" "$(ROOT_SLATE_BIN)"
	chmod +x "$(ROOT_SLATE_BIN)"

run: slate-bin
	./$(ROOT_SLATE_BIN) $(ARGS)

clean-slate-bin:
	rm -f "$(ROOT_SLATE_BIN)"
