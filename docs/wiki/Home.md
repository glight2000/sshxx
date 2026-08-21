# sshxx Wiki

sshxx is a self-hosted collaborative terminal workspace derived from
[ekzhang/sshx](https://github.com/ekzhang/sshx). The daemon owns terminal
processes, the server coordinates encrypted sessions, and browser/Tauri viewers
render the same canvas.

![A complete sshxx workspace with terminal, note, file editor, pages, and collaborators](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-workspace.png)

## Guides

- [Complete feature guide and screenshots](Features)
- [Keyboard and mouse controls](Keyboard-and-Mouse)
- [Architecture, synchronization, persistence, and security](Architecture-and-State)

## Important limitations

- Restarting the daemon restores workspace metadata but recreates shell
  processes; it does not resume the original processes.
- Plain HTTP/WebSocket is intended only for trusted local networks. Use a TLS
  reverse proxy and appropriate access controls on untrusted networks.
- Image paste currently targets local daemon terminals. Remote SSH image
  forwarding needs a separate SFTP/SCP flow.
- AI-agent process detection is not implemented. Attention effects depend on a
  terminal bell or supported OSC notification.
