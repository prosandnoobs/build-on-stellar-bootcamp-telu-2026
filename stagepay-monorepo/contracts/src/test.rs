#[cfg(test)]
mod tests {
    use crate::{NotesContract, NotesContractClient, ContractError};
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        symbol_short, Address, Env, String,
    };

    // ----------------------------------------------------------------
    // SETUP HELPERS
    // ----------------------------------------------------------------

    fn setup_env() -> (Env, NotesContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();

        // Set a starting ledger timestamp so timestamps are non-zero
        env.ledger().set(LedgerInfo {
            timestamp: 1_700_000_000,
            protocol_version: 21,
            sequence_number: 1,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 999,
            min_persistent_entry_ttl: 999,
            max_entry_ttl: 9_999_999,
        });

        let contract_id = env.register_contract(None, NotesContract);
        let client = NotesContractClient::new(&env, &contract_id);
        (env, client)
    }

    fn make_note(
        client: &NotesContractClient,
        owner: &Address,
        title: &str,
        content: &str,
        category: &str,
    ) -> u64 {
        let env = client.env.clone();
        client.create_note(
            owner,
            &String::from_str(&env, title),
            &String::from_str(&env, content),
            &symbol_short!(category),
        )
    }

    // ----------------------------------------------------------------
    // CREATE
    // ----------------------------------------------------------------

    #[test]
    fn test_create_note_returns_id() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);
        let id = make_note(&client, &owner, "Hello", "World", "work");
        assert!(id > 0);
    }

    #[test]
    fn test_create_note_appears_in_get_notes() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);
        let id = make_note(&client, &owner, "Test Title", "Test Content", "personal");

        let notes = client.get_notes(&0, &10);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes.get(0).unwrap().id, id);
    }

    #[test]
    #[should_panic]
    fn test_create_note_empty_title_panics() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);
        client.create_note(
            &owner,
            &String::from_str(&env, ""),
            &String::from_str(&env, "Some content"),
            &symbol_short!("work"),
        );
    }

    // ----------------------------------------------------------------
    // READ
    // ----------------------------------------------------------------

    #[test]
    fn test_get_note_by_id() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);
        let id = make_note(&client, &owner, "Blockchain", "Soroban rocks", "tech");

        let note = client.get_note(&id);
        assert_eq!(note.id, id);
        assert_eq!(note.owner, owner);
    }

    #[test]
    fn test_get_notes_by_owner() {
        let (env, client) = setup_env();
        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);

        make_note(&client, &alice, "Alice 1", "Content", "work");
        make_note(&client, &alice, "Alice 2", "Content", "work");
        make_note(&client, &bob,   "Bob 1",   "Content", "work");

        let alice_notes = client.get_notes_by_owner(&alice);
        assert_eq!(alice_notes.len(), 2);

        let bob_notes = client.get_notes_by_owner(&bob);
        assert_eq!(bob_notes.len(), 1);
    }

    #[test]
    fn test_get_by_category() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);

        make_note(&client, &owner, "Work note",     "...", "work");
        make_note(&client, &owner, "Personal note", "...", "home");
        make_note(&client, &owner, "Work note 2",   "...", "work");

        let work_notes = client.get_by_category(&symbol_short!("work"));
        assert_eq!(work_notes.len(), 2);
    }

    #[test]
    fn test_pagination() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);

        for i in 0..5u32 {
            let title = String::from_str(&env, "Note");
            client.create_note(
                &owner,
                &title,
                &String::from_str(&env, "Content"),
                &symbol_short!("work"),
            );
        }

        let page_0 = client.get_notes(&0, &3);
        let page_1 = client.get_notes(&1, &3);

        assert_eq!(page_0.len(), 3);
        assert_eq!(page_1.len(), 2); // remaining 2
    }

    // ----------------------------------------------------------------
    // UPDATE
    // ----------------------------------------------------------------

    #[test]
    fn test_update_note() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);
        let id = make_note(&client, &owner, "Old Title", "Old Content", "work");

        client.update_note(
            &owner,
            &id,
            &String::from_str(&env, "New Title"),
            &String::from_str(&env, "New Content"),
            &symbol_short!("home"),
        );

        let note = client.get_note(&id);
        assert_eq!(note.title, String::from_str(&env, "New Title"));
        assert_eq!(note.category, symbol_short!("home"));
    }

    #[test]
    #[should_panic]
    fn test_update_note_unauthorized() {
        let (env, client) = setup_env();
        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);
        let id = make_note(&client, &alice, "Alice's Note", "Content", "work");

        // Bob tries to update Alice's note — should panic
        client.update_note(
            &bob,
            &id,
            &String::from_str(&env, "Hacked"),
            &String::from_str(&env, "..."),
            &symbol_short!("work"),
        );
    }

    // ----------------------------------------------------------------
    // PIN
    // ----------------------------------------------------------------

    #[test]
    fn test_toggle_pin() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);
        let id = make_note(&client, &owner, "Important", "Must remember", "work");

        let pinned = client.toggle_pin(&owner, &id);
        assert!(pinned);

        let unpinned = client.toggle_pin(&owner, &id);
        assert!(!unpinned);

        let pinned_notes = client.get_pinned();
        assert_eq!(pinned_notes.len(), 0);
    }

    // ----------------------------------------------------------------
    // DELETE
    // ----------------------------------------------------------------

    #[test]
    fn test_delete_note() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);
        let id = make_note(&client, &owner, "Temp", "Delete me", "work");

        let result = client.delete_note(&owner, &id);
        assert!(result);

        let notes = client.get_notes(&0, &10);
        assert_eq!(notes.len(), 0);
    }

    #[test]
    #[should_panic]
    fn test_delete_nonexistent_panics() {
        let (_, client) = setup_env();
        client.delete_note(&Address::generate(&client.env), &999_u64);
    }

    #[test]
    #[should_panic]
    fn test_delete_unauthorized() {
        let (env, client) = setup_env();
        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);
        let id = make_note(&client, &alice, "Private", "Alice's", "home");

        client.delete_note(&bob, &id); // Bob can't delete Alice's note
    }

    // ----------------------------------------------------------------
    // STATS
    // ----------------------------------------------------------------

    #[test]
    fn test_stats_tracking() {
        let (env, client) = setup_env();
        let owner = Address::generate(&env);

        let id1 = make_note(&client, &owner, "Note 1", "Content", "work");
        let _id2 = make_note(&client, &owner, "Note 2", "Content", "work");
        client.delete_note(&owner, &id1);

        let stats = client.get_stats();
        assert_eq!(stats.total_created, 2);
        assert_eq!(stats.total_deleted, 1);
        assert_eq!(stats.active_notes, 1);
    }
}