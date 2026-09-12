use chrono::NaiveDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BgpState {
    Up,
    Down,
    Unknown,
}

impl BgpState {
    pub fn from_bird_info(info: &str) -> Self {
        match info.trim() {
            "Established" => Self::Up,
            "Active" | "Connect" | "OpenSent" | "OpenConfirm" | "Idle" => Self::Down,
            _ => Self::Unknown,
        }
    }

    pub fn as_oper_state(&self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BirdProtocol {
    pub name: String,
    pub proto: String,
    pub state: BgpState,
    pub neighbor_address: Option<String>,
    pub neighbor_asn: Option<u32>,
    pub prefixes_imported: Option<u32>,
    pub prefixes_exported: Option<u32>,
}

/// Strip BIRD socket protocol response codes from a line
/// "1002-peer_as64500 BGP ..." → Some("peer_as64500 BGP ...")
/// "1006-  BGP state: ..."     → Some("  BGP state: ...")
/// "0000 "                     → None (end of response)
/// Lines without codes pass through unchanged
fn strip_protocol_code(line: &str) -> Option<&str> {
    if line.len() >= 5
        && line.as_bytes()[..4].iter().all(|b| b.is_ascii_digit())
        && (line.as_bytes()[4] == b'-' || line.as_bytes()[4] == b' ')
    {
        if &line[..4] == "0000" {
            return None;
        }
        return Some(&line[5..]);
    }
    Some(line)
}

pub fn parse_protocols(output: &str) -> Vec<BirdProtocol> {
    let mut protocols = Vec::new();
    let mut current: Option<ProtocolBuilder> = None;

    for raw_line in output.lines() {
        let line = match strip_protocol_code(raw_line) {
            Some(l) => l,
            None => continue,
        };

        if line.starts_with("BIRD ") || line.starts_with("Name ") || line.is_empty() {
            continue;
        }

        // New protocol line: starts with non-whitespace
        if !line.starts_with(' ') && !line.starts_with('\t') {
            if let Some(builder) = current.take()
                && let Some(proto) = builder.build()
            {
                protocols.push(proto);
            }
            current = ProtocolBuilder::from_header(line);
            continue;
        }

        // Detail line for current protocol
        if let Some(ref mut builder) = current {
            builder.parse_detail(line);
        }
    }

    // Finalize last protocol
    if let Some(builder) = current.take()
        && let Some(proto) = builder.build()
    {
        protocols.push(proto);
    }

    protocols
}

struct ProtocolBuilder {
    name: String,
    proto: String,
    info: String,
    neighbor_address: Option<String>,
    neighbor_asn: Option<u32>,
    prefixes_imported: Option<u32>,
    prefixes_exported: Option<u32>,
}

impl ProtocolBuilder {
    fn from_header(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        let name = parts[0].to_string();
        let proto = parts[1].to_string();
        let info = if parts.len() >= 6 {
            parts[5..].join(" ")
        } else {
            String::new()
        };

        Some(Self {
            name,
            proto,
            info,
            neighbor_address: None,
            neighbor_asn: None,
            prefixes_imported: None,
            prefixes_exported: None,
        })
    }

    fn parse_detail(&mut self, line: &str) {
        let trimmed = line.trim();

        if let Some(addr) = trimmed.strip_prefix("Neighbor address:") {
            self.neighbor_address = Some(addr.trim().to_string());
        } else if let Some(asn_str) = trimmed.strip_prefix("Neighbor AS:") {
            self.neighbor_asn = asn_str.trim().parse().ok();
        } else if let Some(bgp_state) = trimmed.strip_prefix("BGP state:") {
            self.info = bgp_state.trim().to_string();
        } else if trimmed.starts_with("Routes:") {
            self.parse_routes(trimmed);
        }
    }

    fn parse_routes(&mut self, line: &str) {
        // "Routes:         42 imported, 15 exported, 42 preferred"
        if let Some(routes_part) = line.strip_prefix("Routes:") {
            for part in routes_part.split(',') {
                let tokens: Vec<&str> = part.split_whitespace().collect();
                if tokens.len() >= 2
                    && let Ok(count) = tokens[0].parse::<u32>()
                {
                    match tokens[1] {
                        "imported" => self.prefixes_imported = Some(count),
                        "exported" => self.prefixes_exported = Some(count),
                        _ => {}
                    }
                }
            }
        }
    }

    fn build(self) -> Option<BirdProtocol> {
        if self.proto != "BGP" {
            return None;
        }

        Some(BirdProtocol {
            name: self.name,
            proto: self.proto,
            state: BgpState::from_bird_info(&self.info),
            neighbor_address: self.neighbor_address,
            neighbor_asn: self.neighbor_asn,
            prefixes_imported: self.prefixes_imported,
            prefixes_exported: self.prefixes_exported,
        })
    }
}

/// Parse BIRD uptime in seconds from `show status` output
/// Looks for "Last reboot on <timestamp>" and "Current server time is <timestamp>",
/// calculates the difference
pub fn parse_bird_uptime(output: &str) -> Option<f64> {
    let mut current_time: Option<NaiveDateTime> = None;
    let mut reboot_time: Option<NaiveDateTime> = None;

    for raw_line in output.lines() {
        let line = match strip_protocol_code(raw_line) {
            Some(l) => l.trim(),
            None => continue,
        };

        if let Some(ts) = line.strip_prefix("Current server time is ") {
            current_time = parse_bird_timestamp(ts);
        } else if let Some(ts) = line.strip_prefix("Last reboot on ") {
            reboot_time = parse_bird_timestamp(ts);
        }
    }

    match (current_time, reboot_time) {
        (Some(now), Some(boot)) => {
            let diff = now.signed_duration_since(boot);
            Some(diff.num_milliseconds() as f64 / 1000.0)
        }
        _ => None,
    }
}

/// Parse a BIRD timestamp like "2024-01-15 10:30:00.123"
/// Handles both with and without fractional seconds
fn parse_bird_timestamp(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim();
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

/// Una ruta que un peer le anuncia al route server
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BirdRoute {
    pub prefix: String,
    /// Vacio cuando la ruta no trae el atributo, que no es lo mismo que no
    /// existir: el prefijo igual lo anuncia el peer
    pub as_path: Vec<u32>,
    /// Estandar y grandes en la misma lista, como texto: "64166:65012" y
    /// "64166:1001:1". Van juntas porque quien las lee las lee igual, y el
    /// numero de partes ya dice cual es cual
    pub communities: Vec<String>,
}

/// Convierte "(64166,65012) (64166, 1001, 1)" en ["64166:65012", "64166:1001:1"]
fn parse_communities(valor: &str) -> Vec<String> {
    let mut salida = Vec::new();
    let mut resto = valor;

    while let Some(i) = resto.find('(') {
        let Some(j) = resto[i..].find(')') else { break };
        let partes: Vec<&str> = resto[i + 1..i + j]
            .split(',')
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();
        // Se descarta lo que no sean numeros: el formato de BIRD es conocido, y
        // lo que no calza no se inventa
        if partes.len() >= 2 && partes.iter().all(|p| p.parse::<u32>().is_ok()) {
            salida.push(partes.join(":"));
        }
        resto = &resto[i + j + 1..];
    }

    salida
}

/// Extrae las rutas de un `show route protocol <nombre> all`
///
/// Solo interesan el prefijo y el AS path. El resto de atributos que BIRD
/// imprime se ignora, y el timestamp tambien: BIRD lo da en hora local del
/// route server y a veces sin fecha, asi que el Core lleva su propio reloj
pub fn parse_routes(output: &str) -> Vec<BirdRoute> {
    let mut rutas: Vec<BirdRoute> = Vec::new();

    for raw_line in output.lines() {
        let line = match strip_protocol_code(raw_line) {
            Some(l) => l,
            None => continue,
        };

        // Las continuaciones de un bloque llevan UN espacio donde iria el codigo.
        // La primera ruta del dump cae justo ahi, como continuacion del
        // encabezado de la tabla, asi que sin quitarlo se pierde una ruta por dump
        let line = line.strip_prefix(' ').unwrap_or(line);
        let recortada = line.trim();

        if let Some(v) = recortada
            .strip_prefix("BGP.community:")
            .or_else(|| recortada.strip_prefix("BGP.large_community:"))
        {
            if let Some(ultima) = rutas.last_mut() {
                ultima.communities.extend(parse_communities(v));
            }
            continue;
        }

        if let Some(path) = line.trim().strip_prefix("BGP.as_path:") {
            if let Some(ultima) = rutas.last_mut() {
                ultima.as_path = path
                    .split_whitespace()
                    .filter_map(|t| t.parse::<u32>().ok())
                    .collect();
            }
            continue;
        }

        // Una linea de ruta empieza con el prefijo, sin indentar, y declara el
        // tipo despues. Las continuaciones de un mismo prefijo vienen
        // indentadas y no abren una ruta nueva
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((prefix, resto)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if !prefix.contains('/') || !resto.contains("unicast") {
            continue;
        }

        rutas.push(BirdRoute {
            prefix: prefix.to_string(),
            as_path: Vec::new(),
            communities: Vec::new(),
        });
    }

    rutas
}
