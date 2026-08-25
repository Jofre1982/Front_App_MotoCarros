---
name: "Historia de usuario"
about: "Nueva historia de usuario para el backlog del frontend de MotoYa"
title: "[US] "
labels: []
assignees: ""
---

## Historia de usuario

Como **[rol: pasajero|conductor]**, quiero **[capacidad]**, para **[beneficio]**.

## Contexto
<!-- opcional -->

Por qué importa esta historia, dependencias con otras historias/issues, endpoint(s)
del backend (`Back_App_MotoCarros`) que consume.

## Criterios de aceptación

- [ ] Dado [contexto], cuando [acción], entonces [resultado esperado].
- [ ] Dado [contexto], cuando [acción], entonces [resultado esperado].

## Fuera de alcance
<!-- opcional -->



## Notas técnicas
<!-- opcional -->

- Crate: `moto_core` | `moto_ui` | `web` | `mobile`
- Referencias: endpoint(s) de `/api/v1` involucrados, issues relacionados, decisiones
  en `.claude/STANDARDS.md`.

## Definition of Done

- [ ] Implementado siguiendo `.claude/STANDARDS.md` (separación core/ui/plataforma).
- [ ] Tests en `moto_core` (con HTTP mockeado) en verde.
- [ ] `cargo fmt --all -- --check` sin errores.
- [ ] `cargo clippy --workspace --exclude mobile --all-targets --all-features -- -D warnings` sin errores.
- [ ] No rompe otras pantallas/flujos existentes.

**Estimación:** [talla o puntos]
