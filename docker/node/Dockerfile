FROM ghcr.io/jamietre/crabbit-base:latest

RUN apt-get update -qq && apt-get install -y build-essential \
    && rm -rf /var/lib/apt/lists/*

# pnpm + typescript
RUN npm install -g pnpm typescript ts-node
