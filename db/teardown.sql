PRAGMA foreign_keys = OFF;

DROP TABLE IF EXISTS invoices;
DROP TABLE IF EXISTS invoices_periods_synced;
DROP TABLE IF EXISTS invoices_backfill;
DROP TABLE IF EXISTS taxes;
DROP TABLE IF EXISTS contractors;
DROP TABLE IF EXISTS users;

PRAGMA foreign_keys = ON;