-- Add display-level fields to plans: description and monthly price.
-- Not business logic — just metadata for the UI and customer-facing pages.
ALTER TABLE plans ADD COLUMN description    TEXT;
ALTER TABLE plans ADD COLUMN price_monthly  REAL;
-- Quota type expansion: 'trial' lets the operator create limited test accounts.
-- The quota enforcement treats 'trial' identically to 'permanent' for now —
-- duration_days is what enforces the expiry.  We only add the value to the
-- CHECK constraint here; the application logic uses quota_type to label it.
