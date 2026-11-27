WITH project_ids AS (
    SELECT
        name,
        CASE
            WHEN typeof(id) = 'blob' AND length(id) = 16 THEN lower(
                substr(hex(id), 1, 8) || '-' ||
                substr(hex(id), 9, 4) || '-' ||
                substr(hex(id), 13, 4) || '-' ||
                substr(hex(id), 17, 4) || '-' ||
                substr(hex(id), 21, 12)
            )
            ELSE lower(CAST(id AS TEXT))
        END AS id_text
    FROM projects
),
unique_projects AS (
    SELECT name, MIN(id_text) AS id_text
    FROM project_ids
    GROUP BY name
    HAVING COUNT(*) = 1
)
UPDATE OR IGNORE kpi_snapshots
SET scope = (
    SELECT up.id_text
    FROM unique_projects up
    WHERE up.name = kpi_snapshots.scope
)
WHERE scope <> 'global'
  AND EXISTS (
      SELECT 1 FROM unique_projects up WHERE up.name = kpi_snapshots.scope
  );

DELETE FROM kpi_snapshots
WHERE scope <> 'global'
  AND scope NOT IN (
      SELECT
          CASE
              WHEN typeof(id) = 'blob' AND length(id) = 16 THEN lower(
                  substr(hex(id), 1, 8) || '-' ||
                  substr(hex(id), 9, 4) || '-' ||
                  substr(hex(id), 13, 4) || '-' ||
                  substr(hex(id), 17, 4) || '-' ||
                  substr(hex(id), 21, 12)
              )
              ELSE lower(CAST(id AS TEXT))
          END
      FROM projects
  );
