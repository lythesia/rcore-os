TARGET := riscv64gc-unknown-none-elf
MODE := release
APP_DIR := src/bin
TARGET_DIR := target/$(TARGET)/$(MODE)
APPS := $(wildcard $(APP_DIR)/*.rs) # path/to/xx.rs

ifeq ($(MODE), release)
	MODE_ARG := --release
endif

TEST ?= 0

ifeq ($(TEST), 0) # no test, filter out ch*.rs
	APPS := $(filter-out $(wildcard $(APP_DIR)/ch*.rs), $(wildcard $(APP_DIR)/*.rs))
else ifeq ($(TEST), 1) # use usertests.rs only (to replace initproc)
	APPS := $(APP_DIR)/usertests.rs
else # ch$(T) only, plus initproc, user_shell, halt
	T := $(TEST)
	APPS := $(wildcard $(APP_DIR)/ch$(T)_*.rs)
	APPS += $(APP_DIR)/initproc.rs $(APP_DIR)/user_shell.rs $(APP_DIR)/halt.rs
endif

DROP_APPS := $(filter $(wildcard $(APP_DIR)/ch*), $(wildcard $(APP_DIR)/*)) # exclude ch*.rs

elf: $(APPS)
ifeq ($(TEST), 0)
	@cargo build $(MODE_ARG)
	$(foreach app, $(DROP_APPS), rm -f $(TARGET_DIR)/$(basename $(notdir $(app)));)
else
	@$(foreach app, $(APPS), cargo build $(MODE_ARG) --bin $(basename $(notdir $(app)));)
endif
ifeq ($(TEST), 1)
	@rm -f $(TARGET_DIR)/usertests && mv $(TARGET_DIR)/usertests $(TARGET_DIR)/initproc
endif

build: elf

clean:
	@cargo clean

.PHONY: elf build clean