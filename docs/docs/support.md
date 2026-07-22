# Support

Starlane Notes is self-hosted software. Start with local logs and the self-hosting docs before opening an issue.

## Useful Checks

```bash
docker compose ps
docker compose logs -f starlane-notes
docker compose config
```

For private Gitea packages, verify Docker registry authentication separately from Git or `curl` authentication:

```bash
docker login gitea.2eagles.xyz
docker compose pull
```

## Where to Look

- [Docker setup](self-hosting/docker.md)
- [Environment variables](self-hosting/environment-variables.md)
- [Gitea Container Registry](self-hosting/gitea-container-registry.md)
- [Upgrading](self-hosting/upgrading.md)
