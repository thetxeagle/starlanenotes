# Starlight Docs Portal

Starlane Notes publishes its narrative documentation from `docs/docs/`. The same Markdown drives the Gitea Wiki and the optional static Astro Starlight portal. The API reference remains the running Notes server's Scalar surface at `/api/docs` or `/api/v1/docs`.

## Portal framework

The portal runs on **Astro 5** with the **Starlight** documentation framework. Astro builds the canonical Markdown into static HTML, Starlight provides the navigation and documentation UI, and Pagefind provides static search. The production image serves that output with unprivileged Nginx, so it has no application runtime, database, or persistent storage dependency.

## Local development

From a repository checkout:

```bash
npm ci
npm run docs:dev
```

The portal defaults to `http://localhost:4321`. `npm run docs:sync` copies canonical Markdown into the generated Starlight content directory; do not edit the generated files. Use `npm run docs:build` for a production static build and `npm run docs:preview` to serve that build locally.

Set `STARLANE_DOCS_SITE_URL` at build time when the public portal URL is known:

```bash
STARLANE_DOCS_SITE_URL=https://docs.example.com npm run docs:build
```

## Docker

The portal is a standalone static Nginx container. It has no database, persistent volume, or runtime secrets. It is separate from the main Starlane Notes application container and can be deployed or upgraded independently.

```bash
cd docker
STARLANE_DOCS_PORT=9321 docker compose -f docker-compose.docs.yml up -d --build
curl -fsS http://localhost:9321/ >/dev/null
```

The default port is `9321`. To build the image without Compose:

```bash
docker build -f docker/docs.Dockerfile -t starlane-notes-docs .
docker run --rm -p 9321:8080 starlane-notes-docs
```

The container listens on port `8080` and has a `GET /` health check. It serves only built static files; mount no application data into it.

## Published Gitea image

The `Publish Docs Container` Gitea Actions workflow builds the docs image on pushes to `main`, version branches, and version tags. It publishes:

```text
ghcr.io/hbftechnologies/starlanenotes-docs:latest
ghcr.io/hbftechnologies/starlanenotes-docs:main
ghcr.io/hbftechnologies/starlanenotes-docs:v1.0.12
ghcr.io/hbftechnologies/starlanenotes-docs:sha-<shortsha>
```

For a private package, authenticate Docker first. This is separate from Git or `curl --netrc` authentication:

```bash
docker login gitea.2eagles.xyz
cd docker
docker compose -f docker-compose.docs.yml pull
docker compose -f docker-compose.docs.yml up -d
```

Pin a release instead of tracking `latest` when you need a predictable docs deployment:

```bash
STARLANE_DOCS_IMAGE=ghcr.io/hbftechnologies/starlanenotes-docs:v1.0.12 \
  docker compose -f docker-compose.docs.yml up -d
```

The workflow uses the existing `REGISTRY_USERNAME` and `REGISTRY_TOKEN` Gitea secrets. The token needs package-write access to publish and package-read access for private-image pulls.

## Reverse proxy and API reference

Host the portal on a dedicated hostname such as `docs.example.com`. The Starlight site does not proxy the Notes API. Keep the application on its own hostname, such as `notes.example.com`; API pages link readers to that server's Scalar reference.

```caddy
docs.example.com {
    reverse_proxy localhost:9321
}
```

If the portal and Notes app share a hostname, configure the reverse proxy explicitly so `/api/` routes to the Notes server and all other portal routes go to the docs container. Use separate hostnames when possible to keep deployments simpler.

## Release gate

`npm run docs:validate` checks the documentation contract. `npm run docs:build` validates the Starlight render itself, and CI runs both. Feature, environment-variable, deployment, API, authentication, storage, and sync changes must update the canonical Markdown before either publishing path is used.
