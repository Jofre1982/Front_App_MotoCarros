---
name: "Tarea técnica"
about: "Trabajo del backlog que no es una historia de usuario (setup, refactor, infraestructura, spike)"
title: "[TASK] "
labels: []
assignees: ""
---

## Objetivo

Qué hay que hacer y por qué, en un par de frases. Es una tarea técnica (no una historia
de usuario) porque no tiene un beneficio directo y visible para pasajero/conductor —
por ejemplo, integrar el SDK de mapas, el cliente de WebSockets, o el manejo de JWT.

## Contexto
<!-- opcional -->



## Criterios de aceptación

- [ ] [condición verificable 1]
- [ ] [condición verificable 2]

## Notas técnicas
<!-- opcional -->

- Crate: `moto_core` | `moto_ui` | `web` | `mobile`
- Referencias: enlaces relevantes, decisiones en `.claude/STANDARDS.md`.

## Definition of Done

- [ ] Implementado siguiendo `.claude/STANDARDS.md`.
- [ ] Tests donde aplique, en verde.
- [ ] `cargo fmt --all -- --check` sin errores.
- [ ] `cargo clippy --workspace --exclude mobile --all-targets --all-features -- -D warnings` sin errores.

**Estimación:** [talla o puntos]
