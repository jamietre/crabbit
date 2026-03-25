.PHONY: build web server clean

build: web server

web:
	cd web && npm run build

server: web
	cargo build --release

dev-server:
	cargo run -p crabbit-server -- --config ~/.config/crabbit/server.toml

dev-web:
	cd web && npm run dev

clean:
	rm -rf web/build target
