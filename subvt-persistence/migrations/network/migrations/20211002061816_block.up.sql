CREATE TABLE IF NOT EXISTS sub_block
(
    hash                VARCHAR(66) PRIMARY KEY,
    number              bigint NOT NULL,
    timestamp           bigint,
    author_account_id   VARCHAR(66),
    era_index           bigint NOT NULL,
    epoch_index         bigint NOT NULL,
    parent_hash         VARCHAR(66) NOT NULL,
    state_root          VARCHAR(66) NOT NULL,
    extrinsics_root     VARCHAR(66) NOT NULL,
    is_finalized        BOOLEAN NOT NULL DEFAULT FALSE,
    metadata_version    smallint NOT NULL,
    runtime_version     smallint NOT NULL,
    created_at          TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT now(),
    CONSTRAINT sub_block_fk_account
        FOREIGN KEY (author_account_id)
            REFERENCES sub_account (id)
            ON DELETE CASCADE
            ON UPDATE CASCADE,
    CONSTRAINT sub_block_fk_era
        FOREIGN KEY (era_index)
            REFERENCES sub_era (index)
            ON DELETE CASCADE
            ON UPDATE CASCADE
);

ALTER TABLE sub_account
    ADD CONSTRAINT sub_account_fk_discovered_block
        FOREIGN KEY (discovered_at_block_hash)
            REFERENCES sub_block (hash)
            ON DELETE CASCADE
            ON UPDATE CASCADE;
ALTER TABLE sub_account
    ADD CONSTRAINT sub_account_fk_killed_block
        FOREIGN KEY (killed_at_block_hash)
            REFERENCES sub_block (hash)
            ON DELETE CASCADE
            ON UPDATE CASCADE;

CREATE INDEX sub_block_idx_epoch_index
    ON sub_block (epoch_index);

CREATE INDEX sub_block_idx_era_index
    ON sub_block (era_index);

CREATE INDEX sub_block_idx_number
    ON sub_block (number);

CREATE INDEX sub_block_idx_hash_epoch_index
    ON sub_block (hash, epoch_index);

CREATE INDEX sub_block_idx_author_account_id
    ON sub_block (author_account_id);

CREATE INDEX sub_block_idx_era_index_author_account_id
    ON sub_block (era_index, author_account_id);