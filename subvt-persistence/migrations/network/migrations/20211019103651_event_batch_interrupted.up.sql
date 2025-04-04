CREATE TABLE IF NOT EXISTS sub_event_batch_interrupted
(
    id                      SERIAL PRIMARY KEY,
    block_hash              VARCHAR(66) NOT NULL,
    extrinsic_index         integer,
    event_index             integer NOT NULL,
    item_index              integer NOT NULL,
    dispatch_error_debug    text,
    created_at              TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT now(),
    CONSTRAINT sub_event_batch_interrupted_fk_block
        FOREIGN KEY (block_hash)
            REFERENCES sub_block (hash)
            ON DELETE CASCADE
            ON UPDATE CASCADE
);

CREATE INDEX sub_event_batch_interrupted_idx_block_hash
    ON sub_event_batch_interrupted (block_hash);