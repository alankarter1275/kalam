PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
APPID = app.kalam.Kalam

.PHONY: all build dev test check clean install uninstall

all: build

# `build` and `dev` stay scoped to the app: these are what you run, and
# building the GTK reference viewer and the two demo tools on every `make dev`
# is time spent on binaries you are not about to launch. `make check` and
# `make test` cover the whole workspace, which is where a break in those
# crates gets caught.
build:
	cargo build --release

dev:
	cargo build

# --workspace matters. The root of this workspace is a package (`kalam`), so
# with no `default-members` key Cargo defaults to that one package: a bare
# `cargo test` runs the app's unit tests and silently skips the ~420 tests in
# crates/*/tests and tools/*/tests. Same reason CI passes --workspace.
test:
	cargo test --workspace

check:
	cargo check --workspace

clean:
	cargo clean

install: build
	install -d "$(DESTDIR)$(BINDIR)"
	install -m 755 target/release/kalam "$(DESTDIR)$(BINDIR)/kalam"
	install -d "$(DESTDIR)$(DATADIR)/applications"
	install -m 644 resources/$(APPID).desktop "$(DESTDIR)$(DATADIR)/applications/$(APPID).desktop"
	install -d "$(DESTDIR)$(DATADIR)/icons/hicolor/512x512/apps"
	install -m 644 assets/logo.png "$(DESTDIR)$(DATADIR)/icons/hicolor/512x512/apps/$(APPID).png"

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/kalam"
	rm -f "$(DESTDIR)$(DATADIR)/applications/$(APPID).desktop"
	rm -f "$(DESTDIR)$(DATADIR)/icons/hicolor/512x512/apps/$(APPID).png"
