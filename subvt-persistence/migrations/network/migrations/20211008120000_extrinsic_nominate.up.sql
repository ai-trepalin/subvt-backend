CREATE TABLE IF NOT EXISTS sub_extrinsic_nominate
(
    id                      SERIAL PRIMARY KEY,
    block_hash              VARCHAR(66) NOT NULL,
    extrinsic_index         integer NOT NULL,
    is_nested_call          boolean NOT NULL,
    controller_account_id   VARCHAR(66) NOT NULL,
    is_successful           boolean NOT NULL,
    created_at              TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT now(),
    CONSTRAINT sub_extrinsic_nominate_fk_block
        FOREIGN KEY (block_hash)
            REFERENCES sub_block (hash)
            ON DELETE CASCADE
            ON UPDATE CASCADE,
    CONSTRAINT sub_extrinsic_nominate_fk_controller
        FOREIGN KEY (controller_account_id)
            REFERENCES sub_account (id)
            ON DELETE CASCADE
            ON UPDATE CASCADE
);
