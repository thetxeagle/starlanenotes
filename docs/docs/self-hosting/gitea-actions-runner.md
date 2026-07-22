# Gitea Actions Runner (Docker)

If your workflows stay in `waiting`, your Gitea instance likely has no eligible runner.

Starlane Notes workflows currently target:

- `runs-on: ubuntu-latest`

Your runner must register a matching label.

## 1) Create a registration token

In Gitea:

- `Site Administration -> Actions -> Runners -> Create New Registration Token`

You can also use an org-level or repo-level token if you want tighter scope.

## 2) Prepare host directories

```bash
mkdir -p /opt/gitea-runner/{data,config}
```

## 3) Generate default runner config

```bash
docker run --rm gitea/act_runner:latest act_runner generate-config > /opt/gitea-runner/config/config.yaml
```

## 4) Add runner service to Docker Compose

```yaml
services:
  gitea-runner:
    image: gitea/act_runner:latest
    container_name: gitea-runner
    restart: unless-stopped
    volumes:
      - /opt/gitea-runner/data:/data
      - /opt/gitea-runner/config/config.yaml:/config.yaml
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      GITEA_INSTANCE_URL: "https://gitea.2eagles.xyz"
      GITEA_RUNNER_REGISTRATION_TOKEN: "REPLACE_ME"
    command: >
      sh -c "
      if [ ! -f /data/.runner ]; then
        act_runner register
          --instance $$GITEA_INSTANCE_URL
          --token $$GITEA_RUNNER_REGISTRATION_TOKEN
          --name docker-runner-1
          --labels ubuntu-latest:docker://ghcr.io/catthehacker/ubuntu:act-latest;
      fi;
      act_runner daemon --config /config.yaml
      "
```

## 5) Start and verify

```bash
docker compose up -d gitea-runner
docker logs -f gitea-runner
```

In Gitea UI, verify the runner appears online under Actions runners.

## Troubleshooting

- All jobs show `waiting`: no runner is online, or labels do not match `ubuntu-latest`
- Runner online but jobs still waiting: confirm label includes `ubuntu-latest`
- Intermittent failures: check runner host disk and Docker daemon health

## Optional: use a custom label

If you prefer a custom runner label (for example `starlane-runner`), update workflows:

```yaml
runs-on: starlane-runner
```

Then register the runner with that exact label.
