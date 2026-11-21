.PHONY: help ci dev desktop clean check fmt test clippy payload

help:
	@echo "Tiaki — dev targets"
	@echo "  make check    cargo check --workspace"
	@echo "  make test     cargo test --workspace (178 tests)"
	@echo "  make fmt      cargo fmt --all"
	@echo "  make clippy   cargo clippy --workspace --all-targets -- -D warnings"
	@echo "  make ci       check + fmt --check + clippy + test (what CI runs)"
	@echo "  make dev      ./scripts/dev (prereq check + cargo check)"
	@echo "  make payload  stage bare-metal payload/ (needs Docker)"
	@echo "  make desktop  build+run the Tiaki desktop (Docker, VNC :5900)"
	@echo "  make clean    rm -rf target/ payload/"

ci:
	cargo check --workspace
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

dev:
	./scripts/dev

desktop:
	docker build -f desktop/wayland/Dockerfile -t tiaki-wayland:dev .
	docker run -d --name tiaki-wayland -p 5901:5900 tiaki-wayland:dev
	@echo "VNC: vnc://localhost:5901 (macOS Screen Sharing)"

clean:
	rm -rf target/ payload/

check:
	cargo check --workspace

fmt:
	cargo fmt --all

test:
	cargo test --workspace

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

# Staged OS payload consumed by desktop/install/install.sh. Keep the list in
# sync with desktop/wayland/Dockerfile.
payload:
	rm -rf payload && mkdir -p payload
	cargo build --release -p agentd
	cargo build --release -p agentic-agents
	cargo build --release --manifest-path ui/shell/Cargo.toml
	cp target/release/agentd payload/agentd
	cp target/release/sched-launch payload/sched-launch
	cp ui/shell/target/release/tiaki-shell payload/tiaki-shell
	cp desktop/wayland/tiaki-wallpaper.sh payload/tiaki-wallpaper
	cp desktop/wayland/tiaki-session payload/tiaki-session
	docker build -f desktop/wm/Dockerfile -t tiaki-wm-build desktop/wm >/dev/null
	docker create --name tiaki-wm-extract tiaki-wm-build >/dev/null
	docker cp tiaki-wm-extract:/tiaki-wm payload/tiaki-wm
	docker rm tiaki-wm-extract >/dev/null
	cp -r desktop/wm/assets payload/wm-assets
	cp desktop/bin/tiaki-notify payload/tiaki-notify
	cp desktop/bin/tiaki-t3-open payload/tiaki-t3-open
	cp desktop/desktop.json payload/desktop.json
	cp -r desktop/icons payload/icons
	cp -r desktop/wallpapers payload/wallpapers
	cp desktop/systemd/*.service payload/
	cp desktop/systemd/tiaki-firstboot.sh payload/tiaki-firstboot
	mkdir -p payload/etc-tiaki
	cp desktop/wayland/sway.conf desktop/wayland/wayvnc.conf desktop/wayland/foot.ini payload/etc-tiaki/
	mkdir -p payload/gtk
	cp desktop/wayland/gtk.css payload/gtk/gtk.css
	cp desktop/wayland/gtk-settings.ini payload/gtk/settings.ini
	mkdir -p payload/gtksourceview
	cp desktop/sourceview/tiaki-classic.xml payload/gtksourceview/classic.xml
