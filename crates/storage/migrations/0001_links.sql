-- 링크: 링크 번호와 원본 URL의 연결. 한 번 저장하면 바뀌지 않는다.
CREATE TABLE links (
    -- 링크 번호. GENERATED ALWAYS: 번호는 항상 DB 시퀀스가 발급하고, 클라이언트가 지정할 수 없다.
    -- 시퀀스는 동시 요청에서도 같은 값을 두 번 주지 않고, 롤백·삭제된 값도 다시 쓰지 않는다.
    -- PRIMARY KEY(= UNIQUE + NOT NULL)가 중복을 한 번 더 막는다.
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    original_url TEXT NOT NULL
                 CHECK (original_url ~ '^https?://' AND char_length(original_url) <= 2048),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 생성 후 불변: 애플리케이션 코드에 수정·삭제 경로가 없을 뿐 아니라, DB에서도 막는다.
CREATE FUNCTION reject_link_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'links are immutable: % is not allowed', TG_OP
        USING ERRCODE = 'insufficient_privilege';
END;
$$;

CREATE TRIGGER links_no_update_or_delete
    BEFORE UPDATE OR DELETE ON links
    FOR EACH ROW EXECUTE FUNCTION reject_link_change();

CREATE TRIGGER links_no_truncate
    BEFORE TRUNCATE ON links
    FOR EACH STATEMENT EXECUTE FUNCTION reject_link_change();
