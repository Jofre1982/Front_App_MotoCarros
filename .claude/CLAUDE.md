# MotoYa — Frontend App MotoCarros

## Qué es este proyecto

Frontend multiplataforma en **Rust + Dioxus** para **MotoYa**, un servicio de solicitud de
transporte tipo moto-taxi para un municipio sin cobertura de las apps existentes
(Uber, DiDi, inDriver). Consume el backend de `Jofre1982/Back_App_MotoCarros`
(API REST JSON bajo `/api/v1`, autenticación JWT).

Este repo cubre **dos roles de usuario**: pasajero y conductor. Ambos comparten el
mismo codebase Dioxus, con targets separados para **web (WASM)** y **móvil nativo
(Android/iOS)**.

Estado actual: **esqueleto inicial**, sin funcionalidad de negocio implementada todavía.
Este archivo documenta la intención funcional y las decisiones de arquitectura para
guiar el desarrollo (humano y de los agentes autónomos `front_dev` / `front_reviewer`).

## Stack técnico

- Rust (edición 2021+), toolchain estable vía `rustup`.
- **Dioxus** como framework de UI, multiplataforma:
  - `dioxus-web` (WASM) para el target web, empaquetado como PWA instalable.
  - Target móvil nativo (Android/iOS) compartiendo el mismo árbol de componentes.
- Cliente HTTP hacia el backend: JSON sobre `/api/v1`, JWT en cada request autenticado.
- Testing: `cargo test` sobre el crate/módulo compartido (lógica de estado, cliente API).
  El renderizado real en dispositivo/emulador se valida manualmente, no en CI.
- CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, build del target web
  en cada PR. El build móvil completo (APK/IPA) no es parte del CI por PR — es pesado
  y frágil; se valida manualmente o en un workflow aparte, no bloqueante.

## Riesgo de arquitectura conocido

El lado **conductor** necesita tracking de ubicación en segundo plano (pantalla apagada
o app minimizada). El target web/PWA es débil en esto por las limitaciones de los
navegadores con la geolocalización en background. Si en el piloto esto resulta
insuficiente, la salida es envolver la misma UI Dioxus en un shell nativo delgado
solo para el conductor — sin reescribir la lógica compartida. Ver [STANDARDS.md](STANDARDS.md)
para la separación de capas que hace esto posible sin reescritura mayor.

## Dominio funcional esperado

Consume las funcionalidades ya implementadas en el backend:

- **Autenticación**: login/registro de pasajero y conductor vía JWT.
- **Solicitud de viaje**: pasajero pide un viaje (origen/destino), ve conductores
  disponibles, recibe confirmación.
- **Gestión del viaje (conductor)**: acepta/rechaza solicitudes, actualiza estado
  del viaje, ve ubicación del pasajero.
- **Seguimiento en tiempo real**: ubicación del conductor durante el viaje.
- **Pago y recibo**: pago al finalizar el viaje, visualización del recibo.
- **Calificación**: pasajero califica al conductor al finalizar.
- **Historial**: historial de viajes y ganancias (conductor), historial de viajes (pasajero).

Estos puntos son la intención de negocio; la arquitectura concreta de cada pantalla
se decide al implementar el issue correspondiente, siguiendo las convenciones de
[STANDARDS.md](STANDARDS.md).

## Automatización

Este repo tiene dos agentes autónomos corriendo localmente (Claude Desktop,
`scheduled-tasks`): `front_dev` (desarrolla issues y corrige PRs) y `front_reviewer`
(revisa y mergea a `main` cuando el CI está verde y no hay hilos de review abiertos).
No se espera aprobación humana en el merge — ver las reglas duras de cada rol en
`C:\Users\SARITA\.claude\scheduled-tasks\front_dev\SKILL.md` y `...\front_reviewer\SKILL.md`.
