# Runbook: build the story-time image

This runbook builds the image of the hosted story MCP server
([ADR 0007](../adr/0007-hosted-story-mcp.md)) and pushes it to the image
registry of the coffee cluster. It does not deploy. To deploy, use
coffee-cluster-mcp (`cc_deploy`), which is the only deploy path to the cluster.

GitHub CI builds the same Dockerfile on each pull request, to check it. CI does
not push an image.

## Before you start

- You are on Spark, in a checkout of story-time at the commit to build.
- The commit is on `main`, so that the tag names a merged commit.
- Docker on Spark can push to the plain-HTTP registry `192.168.1.51:30500`.
  `/etc/docker/daemon.json` must list it:

  ```json
  { "insecure-registries": ["192.168.1.51:30500"] }
  ```

  If you change that file, restart Docker: `sudo systemctl restart docker`.

## Build and push

Spark is an arm64 machine and the cluster nodes are amd64, so name the platform.

1.  Set the tag from the commit:

    ```bash
    tag="192.168.1.51:30500/story-time:$(git rev-parse --short HEAD)-amd64"
    ```

2.  Build the image. The build context is the repository root:

    ```bash
    docker buildx build --builder default --platform linux/amd64 --load --network host \
      -f native/story-push/Dockerfile -t "$tag" .
    ```

    `--network host` is necessary on Spark. Without it the build stops at
    `cargo build` with `Could not resolve host: index.crates.io`, because a
    container on the Docker bridge of Spark cannot resolve DNS names
    (seen 2026-11-15). `--builder default` keeps the build on Spark; the
    selected buildx builder there can be a remote one that does not load the
    image into the local Docker.

3.  Check the image before you push it. The command must print the help text:

    ```bash
    docker run --rm --platform linux/amd64 --entrypoint story "$tag" help
    ```

4.  Push the image:

    ```bash
    docker push "$tag"
    ```

5.  Record the tag. `cc_deploy` needs it.

## What the image holds

- `/usr/local/bin/story`, built with `cargo build --release --locked`.
- `git` and `ca-certificates`, because the server runs `git` to clone and fetch.
- The user `story` (UID 10001). The image does not run as root.
- The entrypoint `story mcp --http 0.0.0.0:8080`. The container arguments add
  `--registry`, `--clients`, and the two GitHub App files.

The server writes only under `--data` (default `/data`): the bare clones, one
directory for each commit that a read uses, and the home directory of `git`.
Mount a writable volume there when the root file system is read-only.

## Probes

The image has no `curl`. Use the server binary:

```text
story mcp probe --url http://127.0.0.1:8080/healthz
story mcp probe --url http://127.0.0.1:8080/readyz
```

Each command exits 0 for a 2xx answer and 1 for any other result.

## Deploy

The deploy is story ST-136. It needs the manifests in `team-teddy/coffee-apps`,
the GitHub App, and the Vault values. Give the tag from step 5 to `cc_deploy`,
then confirm with `cc_status`. To go back, use `cc_rollback`. Do not use
`kubectl apply` or a direct push to coffee-apps to deploy.
