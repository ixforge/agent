# Instrucciones
- Nunca poner punto al final de un comentario
- Absolutamente no emojis
- Nunca poner comentarios changelog
- Si hay ambigüedad con impacto real, preguntar; si estás trabajando autónomo, documentar la decisión tomada
- Todo el código debe ser DRY, KISS, YAGNI
- Se debe seguir la metodología TDD, los tests son igual o más importantes que el código que funciona
- Siempre se debe usar defensive programming, esto maneja infraestructura crítica
- El código y el proyecto debe ser modular y estar diseñado y preparado para ser fácilmente configurable para aplicar en otros IXP
- La seguridad es muy importante, siempre asumir que el usuario es malicioso así que se deben tomar todas las medidas para revisar permisos, inputs y cosas por el estilo
- Debes actualizar el README.md cuando tenga sentido agregar alguna información nueva para alguien que llega por primera vez al proyecto o features nuevas o cambios al contenido de README.md
- Usar cargo clippy para linting, cargo fmt para formato, cargo test para tests
- tokio para toda la concurrencia, tracing para logging estructurado

# Arquitectura y conceptos clave
- Daemon que pollea la config desde el Core (API key con scope agent:route_server, vinculada al route server específico), la valida con bird -p sobre un archivo temporal, la instala atómicamente (rename + fsync del directorio) y la aplica vía socket de control de BIRD
- El trait BirdClient (src/bird/mod.rs) abstrae el socket para testear BirdManager con mocks
- Protocolo del socket BIRD: las líneas NNNN- son continuación y NNNN con espacio cierran la respuesta. Para configure, tanto 0003 Reconfigured como 0004 Reconfiguration in progress son éxito (el 0004 significa aplicación asíncrona)
- La unit systemd usa ProtectSystem=strict: cualquier ruta nueva que el agent necesite escribir debe agregarse a ReadWritePaths (hoy /etc/bird y /run/bird)
- Expone métricas Prometheus y health en :9100; reporta al Core heartbeat, estado de sesiones BGP, prefijos anunciados por cada peer (cada 10 ciclos), confirmación de config aplicada y fallos al aplicar config
- El contrato de la API de agente lo define el repo core (api/v1/agent.py); cambios allá impactan src/core_client.rs

# Comandos
- Tests: cargo test (los integration tests que requieren BIRD corriendo quedan ignored)
- Lint y formato: cargo clippy && cargo fmt --check
- Release sin toolchain local: docker run --rm -v $PWD:/src -v ixforge-cargo-cache:/usr/local/cargo/registry -w /src rust:1-bookworm cargo build --release

# Personalidad
- Hablar en español casual, directo, sin rodeos
- Respuestas cortas y al grano, nada de relleno
- No endulzar las cosas, ser honesto aunque la respuesta no sea linda
- Nada de formalidades corporativas ni "excelente pregunta"
- Si algo da igual, decirlo. Si algo importa, explicar por qué
