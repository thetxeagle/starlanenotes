# Gitea Container Registry

Starlane Notes publishes Docker images to the Gitea Container Registry.

The default image name follows Gitea's required pattern:

```text
{registry}/{owner}/{image}:{tag}
```

For this repository:

```text
ghcr.io/hbftechnologies/starlanenotes:latest
```

The separately deployable Starlight documentation portal uses its own image:

```text
ghcr.io/hbftechnologies/starlanenotes-docs:latest
```

The source repository is `hft-applications/StarlaneNotes`, but the container image path is lowercase: `hft-applications/starlanenotes`. Keep Docker image repository names lowercase for compatibility with registry clients and tooling.

The Compose files use:

```yaml
image: ${STARLANE_IMAGE:-ghcr.io/hbftechnologies/starlanenotes:latest}
```

The docs Compose file uses:

```yaml
image: ${STARLANE_DOCS_IMAGE:-ghcr.io/hbftechnologies/starlanenotes-docs:latest}
```

Set `STARLANE_IMAGE` in `.env` if you want to pin a version tag instead of `latest`.

## Authentication

Gitea uses different authentication paths for repository files and container images.

| Task | Command | Credential source |
|---|---|---|
| Clone a private repo | `git clone ...` | Git credential helper or `~/.netrc` |
| Download private raw Compose files | `curl --netrc ...` | `~/.netrc` |
| Pull a private container image | `docker compose pull` | `docker login gitea.2eagles.xyz` |

Docker does not read `~/.netrc`. If the container package is private, run:

```bash
docker login gitea.2eagles.xyz
docker compose pull
docker compose up -d
```

Use a Gitea personal access token instead of a password when the account uses 2FA, OAuth, or automation.

If the repository is private and you download Compose files directly from Gitea, use:

```bash
curl --netrc -fsSLO https://github.com/hbftechnologies/starlanenotes/raw/branch/v1.0.11/docker/docker-compose.yml
curl --netrc -fsSLo .env https://github.com/hbftechnologies/starlanenotes/raw/branch/v1.0.11/docker/.env.example
```

Your `~/.netrc` entry should look like:

```netrc
machine gitea.2eagles.xyz
  login your-username
  password your-gitea-token
```

The token needs read access to the repository for raw file downloads and read access to packages for private image pulls. Exact token scope names depend on the Gitea version and server configuration.

## Publishing Workflow

The repository publishes application images with `.gitea/workflows/publish-container.yml` and docs images with `.gitea/workflows/publish-docs-container.yml`.

Current workflow defaults:

| Setting | Value |
|---|---|
| Registry | `gitea.2eagles.xyz` |
| Image repo | `hft-applications/starlanenotes` |
| Version tag | `${GITHUB_REF_NAME}` for `v*` branches and tags |
| Immutable tag | `sha-${SHORT_SHA}` |
| Floating tag | `latest` on `main` only |

The workflow logs in to the registry with:

```yaml
registry: ${{ env.REGISTRY }}
username: ${{ secrets.REGISTRY_USERNAME }}
password: ${{ secrets.REGISTRY_TOKEN }}
```

Configure those secrets in Gitea before expecting image publishing to work.

Version branches and tags publish version-specific images such as:

```text
ghcr.io/hbftechnologies/starlanenotes:v1.0.11
ghcr.io/hbftechnologies/starlanenotes:sha-476ffc4
```

Merging to `main` publishes:

```text
ghcr.io/hbftechnologies/starlanenotes:main
ghcr.io/hbftechnologies/starlanenotes:latest
ghcr.io/hbftechnologies/starlanenotes:sha-<shortsha>
```

This keeps older version tags available while allowing `latest` to track the current mainline build.

The docs workflow follows the same tagging scheme for `starlanenotes-docs`, but builds `docker/docs.Dockerfile`. Pull it independently with `docker compose -f docker/docker-compose.docs.yml pull`.

Reference: [Gitea Container Registry documentation](https://docs.gitea.com/usage/packages/container).
