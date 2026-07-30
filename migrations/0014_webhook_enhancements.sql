-- Webhook delivery enhancements: auto-disable tracking + improved deliverability

-- Track auto-disabled subscriptions with reason and timestamp
ALTER TABLE webhook_subscriptions
    ADD COLUMN IF NOT EXISTS auto_disabled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS auto_disabled_reason TEXT;

-- Index for listing auto-disabled subscriptions
CREATE INDEX IF NOT EXISTS idx_subs_auto_disabled
    ON webhook_subscriptions (auto_disabled_at) WHERE auto_disabled_at IS NOT NULL;

-- Add columns to track consecutive failures per subscription
ALTER TABLE webhook_subscriptions
    ADD COLUMN IF NOT EXISTS consecutive_failures INTEGER NOT NULL DEFAULT 0;
