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

The current project uses a single required bootstrap migration for the full schema. Since all current tables and guardrails are mandatory for the system to run, they are kept together in `0001_initial_schema.*.sql` instead of being split across additional required files.

Compatibility note:
- fresh databases should be bootstrapped from `0001_initial_schema.*.sql`
- if you already created a database from an older repo state where `0001` had been applied before this consolidation, do one explicit reset/bootstrap so the schema is recreated from the unified file

Local usage:

```bash
./scripts/local/deploy-local-lite.sh --bootstrap-db
```

Later schema changes can be applied with:

```bash
./scripts/local/deploy-local-lite.sh --migrate-db
```

The packaged local flow and the app-host deploy scripts track applied migration filenames in `schema_migrations` and only apply pending `*.up.sql` files when migration execution is explicitly requested.
