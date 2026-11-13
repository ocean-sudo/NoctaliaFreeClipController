# Build, test, and install the FreeClip 2 plugin.
# `make install` copies files into the user plugin directory. It does not
# enable the plugin and it does not delete anything already there.

CARGO ?= cargo
NOCTALIA ?= noctalia
MANIFEST := daemon/Cargo.toml
BIN := freeclip/bin/freeclipd
DATA_HOME := $(if $(XDG_DATA_HOME),$(XDG_DATA_HOME),$(HOME)/.local/share)
DEST := $(DATA_HOME)/noctalia/plugins/freeclip

.PHONY: build test lint check install

build:
	$(CARGO) build --release --manifest-path $(MANIFEST)
	install -d freeclip/bin
	install -m 0755 daemon/target/release/freeclipd $(BIN)

test:
	$(CARGO) test --manifest-path $(MANIFEST)

lint:
	$(NOCTALIA) plugins lint freeclip

check: test lint

install: build
	install -d $(DEST)/lib $(DEST)/translations $(DEST)/bin
	install -m 0644 freeclip/plugin.toml freeclip/README.md $(DEST)/
	install -m 0644 freeclip/service.luau freeclip/widget.luau freeclip/panel.luau freeclip/shortcut.luau $(DEST)/
	install -m 0644 freeclip/lib/client.luau freeclip/lib/labels.luau $(DEST)/lib/
	install -m 0644 freeclip/translations/en.json freeclip/translations/zh-Hans.json $(DEST)/translations/
	install -m 0755 $(BIN) $(DEST)/bin/freeclipd
