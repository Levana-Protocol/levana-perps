use cosmwasm_std::Event;

pub struct NewTracker {
    pub admin: String,
}

impl From<NewTracker> for Event {
    fn from(NewTracker { admin }: NewTracker) -> Self {
        Event::new("levana-new-tracker").add_attribute("admin", admin)
    }
}

pub struct NewCodeIdEvent {
    pub contract_type: String,
    pub code_id: u64,
    pub hash: String,
}

impl From<NewCodeIdEvent> for Event {
    fn from(
        NewCodeIdEvent {
            contract_type,
            code_id,
            hash,
        }: NewCodeIdEvent,
    ) -> Self {
        Event::new("levana-new-code-id")
            .add_attribute("contract-type", contract_type)
            .add_attribute("code-id", code_id.to_string())
            .add_attribute("hash", hash)
    }
}

pub struct InstantiateEvent {
    pub contract_type: String,
    pub code_id: u64,
    pub hash: String,
    pub address: String,
    pub family: String,
    pub sequence: u32,
}

impl From<InstantiateEvent> for Event {
    fn from(
        InstantiateEvent {
            contract_type,
            code_id,
            hash,
            address,
            family,
            sequence,
        }: InstantiateEvent,
    ) -> Self {
        Event::new("levana-instantiate-event")
            .add_attribute("contract-type", contract_type)
            .add_attribute("code-id", code_id.to_string())
            .add_attribute("hash", hash)
            .add_attribute("address", address)
            .add_attribute("family", family)
            .add_attribute("sequence", sequence.to_string())
    }
}

pub struct MigrateEvent {
    pub contract_type: String,
    pub old_code_id: u64,
    pub new_code_id: u64,
    pub old_hash: String,
    pub new_hash: String,
    pub address: String,
    pub family: String,
    pub sequence: u32,
    pub new_migrate_count: u32,
}

impl From<MigrateEvent> for Event {
    fn from(
        MigrateEvent {
            contract_type,
            old_code_id: prev_code_id,
            new_code_id,
            old_hash,
            new_hash,
            address,
            family,
            sequence,
            new_migrate_count,
        }: MigrateEvent,
    ) -> Self {
        Event::new("levana-migrate-event")
            .add_attribute("contract-type", contract_type)
            .add_attribute("old-code-id", prev_code_id.to_string())
            .add_attribute("new-code-id", new_code_id.to_string())
            .add_attribute("old-hash", old_hash)
            .add_attribute("new-hash", new_hash)
            .add_attribute("address", address)
            .add_attribute("family", family)
            .add_attribute("sequence", sequence.to_string())
            .add_attribute("new-migrate-count", new_migrate_count.to_string())
    }
}
