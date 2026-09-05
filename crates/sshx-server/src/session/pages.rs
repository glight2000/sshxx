use super::*;

impl Session {
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
