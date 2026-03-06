---
description: Start or manage a Nitro devnet for manual testing and exploration
---

Manage the Nitro testcontainers devnet. Action: $ARGUMENTS (up | down | status | logs)

For `up`:
// turbo
1. `cargo test -p gashammer-testenv --test smoke -- --nocapture`
   This boots the devnet and prints connection URLs.

For `status`:
// turbo
1. `docker ps --filter "network=nitro-devnet" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"`

For `logs`:
// turbo
1. `docker ps --filter "network=nitro-devnet" -q | xargs -I {} docker logs --tail 50 {}`

For `down`:
1. `docker network ls --filter "name=nitro-devnet" -q | xargs -I {} docker network rm {}`
   This tears down the devnet and all its containers.
