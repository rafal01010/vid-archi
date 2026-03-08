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
./scripts/local/deploy-local-lite.sh --bootstrap-db
```

Later schema changes can be applied with:

```bash
./scripts/local/deploy-local-lite.sh --migrate-db
```

The packaged local and STG-Lite deploy scripts track applied migration filenames in `schema_migrations` and only apply pending `*.up.sql` files when migration execution is explicitly requested.
