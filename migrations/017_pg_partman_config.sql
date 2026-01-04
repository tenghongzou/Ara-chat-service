-- Configure pg_partman for automatic partition management
-- This migration runs after pg_partman extension is installed via Docker init script

-- Verify pg_partman extension exists
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_partman') THEN
        RAISE EXCEPTION 'pg_partman extension is not installed. Please ensure PostgreSQL is started with pg_partman support.';
    END IF;
END
$$;

-- Configure pg_partman for messages table
-- - Daily partitions (partition_key is DATE type)
-- - Pre-create 7 days of partitions ahead
-- - Keep partitions indefinitely (no retention policy - messages are permanent)
SELECT partman.create_parent(
    p_parent_table := 'public.messages',
    p_control := 'partition_key',
    p_type := 'native',
    p_interval := '1 day',
    p_premake := 7
);

-- Update part_config to disable automatic retention (keep all partitions)
UPDATE partman.part_config
SET retention = NULL,
    retention_keep_table = true,
    retention_keep_index = true
WHERE parent_table = 'public.messages';

-- Configure pg_partman for message_reactions table
-- Same configuration as messages table
SELECT partman.create_parent(
    p_parent_table := 'public.message_reactions',
    p_control := 'message_partition_key',
    p_type := 'native',
    p_interval := '1 day',
    p_premake := 7
);

-- Update part_config to disable automatic retention
UPDATE partman.part_config
SET retention = NULL,
    retention_keep_table = true,
    retention_keep_index = true
WHERE parent_table = 'public.message_reactions';

-- Run initial maintenance to create partitions for the next 7 days
SELECT partman.run_maintenance();

-- Log configuration summary
DO $$
DECLARE
    config_count INT;
BEGIN
    SELECT COUNT(*) INTO config_count FROM partman.part_config;
    RAISE NOTICE 'pg_partman configured for % partitioned tables', config_count;
END
$$;
