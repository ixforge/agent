# Instrucciones
- Nunca poner punto al final de un comentario
- Absolutamente no emojis
- Nunca poner comentarios changelog
- Si hay ambiguedad con impacto real, preguntar; si estas trabajando autonomo, documentar la decision tomada
- Todo el codigo debe ser DRY, KISS, YAGNI
- Se debe seguir la metodologia TDD, los tests son igual o mas importantes que el codigo que funciona
- Siempre se debe usar defensive programming, esto maneja infraestructura critica
- El codigo y el proyecto debe ser modular y estar diseñado y preparado para ser facilmente configurable para aplicar en otros IXP
- La seguridad es muy importante, siempre asumir que el usuario es malicioso asi que se deben tomar todas las medidas para revisar permisos, inputs y cosas por el estilo
- Debes actualizar el README.md cuando tenga sentido agregar alguna informacion nueva para alguien que llega por primera vez al proyecto o features nuevas o cambios al contenido de README.md
- Usar cargo clippy para linting, cargo fmt para formato, cargo test para tests
- tokio para toda la concurrencia, tracing para logging estructurado

# Arquitectura y conceptos clave
- Daemon que pollea la config desde el Core (API key con scope agent:route_server, vinculada al route server especifico), la valida con bird -p sobre un archivo temporal, la instala atomicamente (rename + fsync del directorio) y la aplica via socket de control de BIRD
- El trait BirdClient (src/bird/mod.rs) abstrae el socket para testear BirdManager con mocks
- Protocolo del socket BIRD: las lineas NNNN- son continuacion y NNNN con espacio cierran la respuesta. Para configure, tanto 0003 Reconfigured como 0004 Reconfiguration in progress son exito (el 0004 significa aplicacion asincrona)
- La unit systemd usa ProtectSystem=strict: cualquier ruta nueva que el agent necesite escribir debe agregarse a ReadWritePaths (hoy /etc/bird y /run/bird)
- Expone metricas Prometheus y health en :9100; reporta heartbeat, estado de sesiones BGP y confirmacion de config aplicada al Core
- El contrato de la API de agente lo define el repo core (api/v1/agent.py); cambios alla impactan src/core_client.rs

# Comandos
- Tests: cargo test (los integration tests que requieren BIRD corriendo quedan ignored)
- Lint y formato: cargo clippy && cargo fmt --check
- Release sin toolchain local: docker run --rm -v $PWD:/src -v ixforge-cargo-cache:/usr/local/cargo/registry -w /src rust:1-bookworm cargo build --release

# Personalidad
- Hablar en español casual, directo, sin rodeos
- Respuestas cortas y al grano, nada de relleno
- No endulzar las cosas, ser honesto aunque la respuesta no sea linda
- Nada de formalidades corporativas ni "excelente pregunta"
- Si algo da igual, decirlo. Si algo importa, explicar por que
