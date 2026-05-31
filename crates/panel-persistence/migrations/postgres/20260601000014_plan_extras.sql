-- Add display-level fields to plans: description and monthly price.
ALTER TABLE plans ADD COLUMN IF NOT EXISTS description    TEXT;
ALTER TABLE plans ADD COLUMN IF NOT EXISTS price_monthly  DOUBLE PRECISION;
