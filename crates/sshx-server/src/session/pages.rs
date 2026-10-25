use super::*;

impl Session {
    /// Move a page before a stable anchor (or to the end). Mutate the current
    /// registry atomically rather than replacing it with a client's stale list.
    pub fn reorder_page(&self, id: u32, before_id: Option<u32>) -> Result<()> {
        let mut result = Ok(());
        let changed = self.pages.send_if_modified(|pages| {
            let Some(from) = pages.iter().position(|page| page.id == id) else {
                result = Err(anyhow::anyhow!("cannot reorder missing page"));
                return false;
            };
            let target = match before_id {
                Some(before) => match pages.iter().position(|page| page.id == before) {
                    Some(index) => index,
                    None => {
                        result = Err(anyhow::anyhow!("cannot reorder before missing page"));
                        return false;
                    }
                },
                None => pages.len(),
            };
            let to = if target > from { target - 1 } else { target };
            if from == to {
                return false;
            }
            let page = pages.remove(from);
            pages.insert(to, page);
            true
        });
        if changed {
            self.workspace_changed();
        }
        result
    }

    /// Delete a page under its registry's write lock. Creation and cross-page
    /// moves hold a read guard until their mutations finish, so no late insert
    /// can resurrect components on the removed page.
    pub fn delete_page(&self, id: u32) -> Result<Vec<Sid>> {
        let mut result = Err(anyhow::anyhow!("cannot delete missing page"));
        self.pages.send_if_modified(|pages| {
            if !pages.iter().any(|page| page.id == id) {
                return false;
            }
            if pages.len() <= 1 {
                result = Err(anyhow::anyhow!("at least one page must remain"));
                return false;
            }
            let shell_ids: Vec<_> = self
                .source
                .borrow()
                .iter()
                .filter(|(_, shell)| shell.page_id == id)
                .map(|(id, _)| *id)
                .collect();
            for shell_id in &shell_ids {
                // Also cleans up file windows tied to this terminal, including
                // those presented on a different page, and their note links.
                if let Err(error) = self.close_shell(*shell_id) {
                    result = Err(error);
                    return false;
                }
            }
            let note_ids: Vec<_> = self
                .notes
                .borrow()
                .iter()
                .filter(|(_, note)| note.page_id == id)
                .map(|(id, _)| *id)
                .collect();
            for note_id in note_ids {
                // A concurrent individual close is harmless.
                let _ = self.close_note(note_id, id);
            }
            let file_ids: Vec<_> = self
                .file_windows
                .borrow()
                .iter()
                .filter(|(_, window)| window.page_id == id)
                .map(|(id, _)| *id)
                .collect();
            for file_id in file_ids {
                let _ = self.close_file_window(file_id, id);
            }
            self.custom_windows.send_modify(|windows| {
                windows.retain(|(_, window)| window.page_id != id);
            });
            pages.retain(|page| page.id != id);
            result = Ok(shell_ids);
            true
        });
        if result.is_ok() {
            self.workspace_changed();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reordering_preserves_page_identity_and_survives_both_restore_paths() {
        let session = super::super::tests::session();
        let second = session.create_page("Second".into()).unwrap();
        let third = session.create_page("Third".into()).unwrap();
        session.add_note(Sid(7), (12, 34), second, None).unwrap();
        session.reorder_page(third, Some(1)).unwrap();
        session.reorder_page(1, None).unwrap();
        let expected = vec![third, second, 1];
        assert_eq!(
            session
                .pages
                .borrow()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            expected
        );
        let workspace = session.workspace_state();
        assert_eq!(workspace.notes[0].page_id, second);
        assert_eq!(workspace.notes[0].x, 12);
        assert_eq!(workspace.pages[0].name, "Third");
        let restored = super::super::tests::session();
        restored.restore_workspace(workspace).unwrap();
        assert_eq!(*restored.pages.borrow(), *session.pages.borrow());
        let restored = Session::restore(&session.snapshot().unwrap()).unwrap();
        assert_eq!(*restored.pages.borrow(), *session.pages.borrow());
        assert!(restored.create_page("Next".into()).unwrap() > third);
    }

    #[test]
    fn invalid_and_noop_moves_do_not_publish_or_change_the_registry() {
        let session = super::super::tests::session();
        let second = session.create_page("Second".into()).unwrap();
        let watcher = session.pages.subscribe();
        assert!(session.reorder_page(999, Some(1)).is_err());
        assert!(session.reorder_page(1, Some(999)).is_err());
        session.reorder_page(1, Some(1)).unwrap();
        session.reorder_page(1, Some(second)).unwrap();
        session.reorder_page(second, None).unwrap();
        assert!(!watcher.has_changed().unwrap());
        assert_eq!(
            session
                .pages
                .borrow()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![1, second]
        );
    }

    #[test]
    fn anchored_move_preserves_concurrent_creates_and_renames() {
        let session = super::super::tests::session();
        let second = session.create_page("Second".into()).unwrap();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                session.create_page("New".into()).unwrap();
                session.rename_page(second, "Renamed".into()).unwrap();
            });
            barrier.wait();
            session.reorder_page(second, Some(1)).unwrap();
        });
        let pages = session.pages.borrow();
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].id, second);
        assert_eq!(pages[0].name, "Renamed");
        assert_eq!(pages[1].id, 1);
        assert_eq!(pages[2].name, "New");
    }

    #[test]
    fn concurrent_creation_cannot_leave_notes_on_a_deleted_page() {
        let session = super::super::tests::session();
        let page = session.create_page("Race".into()).unwrap();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                for id in 1..100 {
                    let _ = session.add_note(Sid(id), (0, 0), page, None);
                    let _ = session.workspace_state();
                }
            });
            barrier.wait();
            session.delete_page(page).unwrap();
        });
        assert!(session.workspace_state().notes.is_empty());
    }

    #[test]
    fn deletion_cleans_components_links_editors_and_persistence() {
        let session = super::super::tests::session();
        let page = session.create_page("Temporary".into()).unwrap();
        session
            .add_shell(
                Sid(1),
                (0, 0),
                page,
                (24, 80),
                (640, 480),
                (String::new(), String::new(), String::new()),
            )
            .unwrap();
        session.add_note(Sid(2), (0, 0), page, None).unwrap();
        session.add_note(Sid(3), (0, 0), 1, None).unwrap();
        session
            .open_file_window(
                Sid(4),
                Sid(1),
                page,
                "/tmp".into(),
                "Files".into(),
                0,
                0,
                800,
                480,
            )
            .unwrap();
        session
            .add_custom_window(Sid(5), (0, 0), (640, 480), page)
            .unwrap();
        session.note_editors.write().insert(Sid(2), Uid(1));
        session.notes.send_modify(|notes| {
            let note = &mut notes.iter_mut().find(|(id, _)| *id == Sid(3)).unwrap().1;
            note.linked_note_ids.push(Sid(2));
            note.linked_shell_ids.push(Sid(1));
            note.linked_file_window_ids.push(Sid(4));
        });
        assert_eq!(session.delete_page(page).unwrap(), vec![Sid(1)]);
        let workspace = session.workspace_state();
        assert_eq!(workspace.pages.len(), 1);
        assert!(workspace.shells.is_empty());
        assert!(workspace.file_windows.is_empty());
        assert!(workspace.custom_windows.is_empty());
        assert_eq!(workspace.notes.len(), 1);
        assert!(workspace.notes[0].linked_shell_ids.is_empty());
        assert!(workspace.notes[0].linked_note_ids.is_empty());
        assert!(workspace.notes[0].linked_file_window_ids.is_empty());
        assert!(session.note_editors.read().is_empty());
        assert!(session.shells.read()[&Sid(1)].closed);
        session
            .add_data(Sid(1), Bytes::from_static(b"late output"), 0)
            .unwrap();
        assert!(session.shells.read()[&Sid(1)].data.is_empty());
        assert!(session
            .add_shell(
                Sid(7),
                (0, 0),
                page,
                (24, 80),
                (640, 480),
                (String::new(), String::new(), String::new())
            )
            .is_err());
        session.close_shell(Sid(7)).unwrap();
        assert!(session.delete_page(page).is_err());
        assert!(session.delete_page(1).is_err());
        assert!(session.create_page("Replacement".into()).unwrap() > page);
        assert!(session.add_note(Sid(6), (0, 0), page, None).is_err());
    }
}
