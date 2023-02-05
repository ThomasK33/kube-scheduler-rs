_default:
	just --list

# Run local development environment
run: start-k3d start-devspace
# Stop local development environment
stop: stop-devspace stop-k3d

# Run mkdocs locally
docs:
	cargo about generate about.hbs > docs/license.md
	mkdocs serve

# --- k3d ---

# Setup local k3d cluster and registry
start-k3d:
	k3d registry create default-registry.localhost --port 9090
	k3d cluster create default --servers 3 --registry-use k3d-default-registry.localhost:9090

	kubectl create ns devspace

# Delete local k3d cluster and registry
stop-k3d:
	k3d cluster delete default
	k3d registry delete default-registry.localhost

# --- DevSpace ---

# Start a DevSpace session
start-devspace:
	devspace use namespace devspace
	devspace dev

# Purge DevSpace resurces
stop-devspace:
	devspace purge
