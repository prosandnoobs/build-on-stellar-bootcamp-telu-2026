#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror, panic_with_error,
    symbol_short, Address, Env, String, Symbol, Vec,
};

// ================================================================
// ERROR TYPES
// ================================================================

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    NoteNotFound    = 1,
    Unauthorized    = 2,
    InvalidInput    = 3,
    StorageLimitHit = 4,
}

// ================================================================
// DATA STRUCTURES
// ================================================================

#[contracttype]
#[derive(Clone, Debug)]
pub struct Note {
    pub id:         u64,
    pub owner:      Address,
    pub title:      String,
    pub content:    String,
    pub category:   Symbol,
    pub is_pinned:  bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ContractStats {
    pub total_created: u64,
    pub total_deleted: u64,
    pub active_notes:  u32,
}

// ================================================================
// STORAGE KEYS
// ================================================================

#[contracttype]
pub enum StorageKey {
    AllNotes,
    Stats,
}

// ================================================================
// CONSTANTS
// ================================================================

const MAX_NOTES:    u32    = 500;
const STORAGE_TTL:  u32    = 10_000; // ledger TTL bump
const BUMP_AMOUNT:  u32    = 5_000;

// Events
const EVT_CREATED:  Symbol = symbol_short!("created");
const EVT_UPDATED:  Symbol = symbol_short!("updated");
const EVT_DELETED:  Symbol = symbol_short!("deleted");
const EVT_PINNED:   Symbol = symbol_short!("pinned");

// ================================================================
// CONTRACT
// ================================================================

#[contract]
pub struct NotesContract;

#[contractimpl]
impl NotesContract {

    // ------------------------------------------------------------
    // INTERNAL HELPERS
    // ------------------------------------------------------------

    fn load_notes(env: &Env) -> Vec<Note> {
        env.storage()
            .instance()
            .get(&StorageKey::AllNotes)
            .unwrap_or(Vec::new(env))
    }

    fn save_notes(env: &Env, notes: &Vec<Note>) {
        env.storage().instance().set(&StorageKey::AllNotes, notes);
        env.storage().instance().extend_ttl(BUMP_AMOUNT, STORAGE_TTL);
    }

    fn load_stats(env: &Env) -> ContractStats {
        env.storage()
            .instance()
            .get(&StorageKey::Stats)
            .unwrap_or(ContractStats {
                total_created: 0,
                total_deleted: 0,
                active_notes:  0,
            })
    }

    fn save_stats(env: &Env, stats: &ContractStats) {
        env.storage().instance().set(&StorageKey::Stats, stats);
    }

    // ------------------------------------------------------------
    // READ — GET ALL (paginated)
    // ------------------------------------------------------------

    /// Returns notes for a given page. page starts at 0.
    pub fn get_notes(env: Env, page: u32, page_size: u32) -> Vec<Note> {
        let notes = Self::load_notes(&env);
        let start  = page * page_size;
        let end    = (start + page_size).min(notes.len());
        let mut result = Vec::new(&env);
        for i in start..end {
            result.push_back(notes.get(i).unwrap());
        }
        result
    }

    // ------------------------------------------------------------
    // READ — GET BY OWNER
    // ------------------------------------------------------------

    pub fn get_notes_by_owner(env: Env, owner: Address) -> Vec<Note> {
        let notes = Self::load_notes(&env);
        let mut result = Vec::new(&env);
        for i in 0..notes.len() {
            let note = notes.get(i).unwrap();
            if note.owner == owner {
                result.push_back(note);
            }
        }
        result
    }

    // ------------------------------------------------------------
    // READ — GET BY ID
    // ------------------------------------------------------------

    pub fn get_note(env: Env, id: u64) -> Note {
        let notes = Self::load_notes(&env);
        for i in 0..notes.len() {
            let note = notes.get(i).unwrap();
            if note.id == id {
                return note;
            }
        }
        panic_with_error!(&env, ContractError::NoteNotFound);
    }

    // ------------------------------------------------------------
    // READ — GET BY CATEGORY
    // ------------------------------------------------------------

    pub fn get_by_category(env: Env, category: Symbol) -> Vec<Note> {
        let notes = Self::load_notes(&env);
        let mut result = Vec::new(&env);
        for i in 0..notes.len() {
            let note = notes.get(i).unwrap();
            if note.category == category {
                result.push_back(note);
            }
        }
        result
    }

    // ------------------------------------------------------------
    // READ — GET PINNED NOTES
    // ------------------------------------------------------------

    pub fn get_pinned(env: Env) -> Vec<Note> {
        let notes = Self::load_notes(&env);
        let mut result = Vec::new(&env);
        for i in 0..notes.len() {
            let note = notes.get(i).unwrap();
            if note.is_pinned {
                result.push_back(note);
            }
        }
        result
    }

    // ------------------------------------------------------------
    // READ — CONTRACT STATS
    // ------------------------------------------------------------

    pub fn get_stats(env: Env) -> ContractStats {
        Self::load_stats(&env)
    }

    // ------------------------------------------------------------
    // WRITE — CREATE NOTE
    // Returns the new note's ID.
    // ------------------------------------------------------------

    pub fn create_note(
        env:      Env,
        owner:    Address,
        title:    String,
        content:  String,
        category: Symbol,
    ) -> u64 {
        // Auth: caller must be the owner
        owner.require_auth();

        // Validate
        if title.len() == 0 || content.len() == 0 {
            panic_with_error!(&env, ContractError::InvalidInput);
        }

        let mut notes = Self::load_notes(&env);

        if notes.len() >= MAX_NOTES {
            panic_with_error!(&env, ContractError::StorageLimitHit);
        }

        let id        = env.prng().gen::<u64>();
        let timestamp = env.ledger().timestamp();

        let note = Note {
            id,
            owner:      owner.clone(),
            title,
            content,
            category,
            is_pinned:  false,
            created_at: timestamp,
            updated_at: timestamp,
        };

        notes.push_back(note);
        Self::save_notes(&env, &notes);

        // Update stats
        let mut stats = Self::load_stats(&env);
        stats.total_created += 1;
        stats.active_notes   = notes.len();
        Self::save_stats(&env, &stats);

        // Emit event
        env.events().publish((EVT_CREATED, owner), id);

        id
    }

    // ------------------------------------------------------------
    // WRITE — UPDATE NOTE (owner only)
    // ------------------------------------------------------------

    pub fn update_note(
        env:      Env,
        caller:   Address,
        id:       u64,
        title:    String,
        content:  String,
        category: Symbol,
    ) -> bool {
        caller.require_auth();

        let mut notes = Self::load_notes(&env);

        for i in 0..notes.len() {
            let mut note = notes.get(i).unwrap();
            if note.id == id {
                if note.owner != caller {
                    panic_with_error!(&env, ContractError::Unauthorized);
                }

                // Only overwrite non-empty fields
                if title.len() > 0   { note.title   = title; }
                if content.len() > 0 { note.content = content; }
                note.category   = category;
                note.updated_at = env.ledger().timestamp();

                notes.set(i, note);
                Self::save_notes(&env, &notes);
                env.events().publish((EVT_UPDATED, caller), id);
                return true;
            }
        }
        panic_with_error!(&env, ContractError::NoteNotFound);
    }

    // ------------------------------------------------------------
    // WRITE — TOGGLE PIN (owner only)
    // Returns new pinned state.
    // ------------------------------------------------------------

    pub fn toggle_pin(env: Env, caller: Address, id: u64) -> bool {
        caller.require_auth();

        let mut notes = Self::load_notes(&env);

        for i in 0..notes.len() {
            let mut note = notes.get(i).unwrap();
            if note.id == id {
                if note.owner != caller {
                    panic_with_error!(&env, ContractError::Unauthorized);
                }
                note.is_pinned = !note.is_pinned;
                let new_state  = note.is_pinned;
                notes.set(i, note);
                Self::save_notes(&env, &notes);
                env.events().publish((EVT_PINNED, caller), (id, new_state));
                return new_state;
            }
        }
        panic_with_error!(&env, ContractError::NoteNotFound);
    }

    // ------------------------------------------------------------
    // WRITE — DELETE NOTE (owner only)
    // ------------------------------------------------------------

    pub fn delete_note(env: Env, caller: Address, id: u64) -> bool {
        caller.require_auth();

        let mut notes = Self::load_notes(&env);

        for i in 0..notes.len() {
            let note = notes.get(i).unwrap();
            if note.id == id {
                if note.owner != caller {
                    panic_with_error!(&env, ContractError::Unauthorized);
                }
                notes.remove(i);
                Self::save_notes(&env, &notes);

                let mut stats = Self::load_stats(&env);
                stats.total_deleted += 1;
                stats.active_notes   = notes.len();
                Self::save_stats(&env, &stats);

                env.events().publish((EVT_DELETED, caller), id);
                return true;
            }
        }
        panic_with_error!(&env, ContractError::NoteNotFound);
    }
}

mod test;