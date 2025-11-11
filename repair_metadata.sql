-- Emergency Metadata Table Repair Script
-- Use this to fix corrupted column_index values in metadata tables
-- 
-- Usage: 
--   1. Close the application
--   2. Open the .db file in SQLite browser or command line
--   3. Run this script
--   4. Restart the application

-- Example: Repair ShipUnits_Metadata
-- Adjust table name as needed

BEGIN TRANSACTION;

-- Step 1: Create a temporary backup
CREATE TEMPORARY TABLE temp_backup AS 
SELECT * FROM ShipUnits_Metadata;

-- Step 2: Drop the corrupted table
DROP TABLE ShipUnits_Metadata;

-- Step 3: Recreate with proper schema
CREATE TABLE ShipUnits_Metadata (
    column_index INTEGER PRIMARY KEY NOT NULL,
    column_name TEXT NOT NULL UNIQUE,
    data_type TEXT,
    validator_type TEXT,
    validator_config TEXT,
    ai_context TEXT,
    filter_expr TEXT,
    ai_enable_row_generation INTEGER DEFAULT 0,
    ai_include_in_send INTEGER DEFAULT 1,
    deleted INTEGER DEFAULT 0
);

-- Step 4: Re-insert data with inferred proper indices
-- This assigns column_index values 0, 1, 2, 3... based on row order
INSERT INTO ShipUnits_Metadata (
    column_index, 
    column_name, 
    data_type, 
    validator_type, 
    validator_config,
    ai_context,
    filter_expr,
    ai_enable_row_generation,
    ai_include_in_send,
    deleted
)
SELECT 
    ROW_NUMBER() OVER (ORDER BY rowid) - 1 AS column_index,
    column_name,
    data_type,
    validator_type,
    validator_config,
    ai_context,
    filter_expr,
    ai_enable_row_generation,
    ai_include_in_send,
    deleted
FROM temp_backup
WHERE deleted IS NULL OR deleted = 0
ORDER BY rowid;

-- Also re-insert deleted columns at the end
INSERT INTO ShipUnits_Metadata (
    column_index, 
    column_name, 
    data_type, 
    validator_type, 
    validator_config,
    ai_context,
    filter_expr,
    ai_enable_row_generation,
    ai_include_in_send,
    deleted
)
SELECT 
    (SELECT MAX(column_index) FROM ShipUnits_Metadata) + ROW_NUMBER() OVER (ORDER BY rowid) AS column_index,
    column_name,
    data_type,
    validator_type,
    validator_config,
    ai_context,
    filter_expr,
    ai_enable_row_generation,
    ai_include_in_send,
    deleted
FROM temp_backup
WHERE deleted = 1
ORDER BY rowid;

COMMIT;

-- Verify the repair
SELECT 
    column_index,
    column_name,
    data_type,
    deleted,
    typeof(column_index) as column_index_type
FROM ShipUnits_Metadata
ORDER BY column_index;

-- The column_index_type should show "integer" for all rows
