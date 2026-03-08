# Database Migrations

This directory stores SQL-first migrations for the video streaming metadata database.

Current migration strategy:
- migrations are plain PostgreSQL SQL files
- `*.up.sql` files apply schema changes
- `*.down.sql` files define the rollback for the matching migration
- the local runner applies `*.up.sql` files in lexical order

Current files:
- `0001_initial_schema.up.sql`
- `0001_initial_schema.down.sql`
- `0002_video_state_guardrails.up.sql`
- `0002_video_state_guardrails.down.sql`

Local usage:

```bash
./scripts/local/start-infra.sh
./scripts/local/db-migrate.sh
```

Backup before risky changes:

```bash
./scripts/local/db-backup.sh
```
