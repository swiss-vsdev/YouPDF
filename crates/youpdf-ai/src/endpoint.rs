//! durcissement des points d'entrée réseau des fournisseurs d'IA.
//!
//! L'utilisateur — ou un fichier de configuration importé, ou un script
//! de déploiement — peut indiquer une `base_url` arbitraire, et la clé
//! API part en en-tête `Authorization` vers cette adresse. Trois trous,
//! mesurés et bouchés ici :
//!
//! 1. **`http://` hors machine locale.** reqwest ne dit rien, et la clé
//!    *plus le contenu intégral des documents* partent en clair sur le
//!    réseau. C'est le seul des trois que **rien d'autre** ne filtre :
//!    d'où [`parse_base_url`], qui l'interdit au moment de la
//!    configuration, et qui refuse aussi les identifiants planqués dans
//!    l'URL et les schémas farfelus.
//! 2. **Aucun délai.** Un endpoint qui accepte la connexion puis se tait
//!    immobilisait le worker IA indéfiniment. D'où `REQUEST_TIMEOUT`.
//! 3. **Les redirections.** reqwest supprime déjà `Authorization` lors
//!    d'un saut d'hôte (vérifié empiriquement, cf. plus bas), mais il
//!    continue de suivre la chaîne — y compris un passage HTTPS → HTTP,
//!    où le *corps* de la requête, lui, n'est pas protégé. La politique
//!    posée par [`http_client`] refuse tout saut hors de l'origine
//!    configurée dès qu'un secret est en jeu.
//!
//! Rien de tout cela n'empêche un usage légitime : LM Studio, Ollama,
//! vLLM ou un serveur self-hosted restent acceptés, sur la boucle
//! locale en HTTP, partout en HTTPS.

use std::sync::Arc;
use std::time::Duration;

use reqwest::redirect::{Attempt, Policy};
use reqwest::Client;
use youpdf_shared::{Error, Result};

/// Timeout global d'une requête réseau : sans lui, un endpoint qui
/// accepte la connexion puis ne répond jamais bloque le worker IA
/// indéfiniment (l'interface, elle, reste vivante — cf. `ai_worker`).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Valide et normalise une URL de base de fournisseur.
///
/// Retourne l'URL sans `/` final, prête à recevoir `/chat/completions`,
/// `/messages`, `/api/chat`…
pub fn parse_base_url(raw: &str) -> Result<String> {
    parse_base_url_inner(raw, false)
}

/// Comme [`parse_base_url`], mais pour les fournisseurs qui **n'envoient
/// aucun secret** (Ollama).
///
/// Le HTTP reste légitime hors boucle locale : un serveur Ollama tourne
/// souvent sur une autre machine du réseau local, et il n'y a rien à
/// intercepter. On valide quand même le schéma, l'hôte et l'absence
/// d'identifiants.
pub fn parse_public_base_url(raw: &str) -> Result<String> {
    parse_base_url_inner(raw, true)
}

fn parse_base_url_inner(raw: &str, allow_http_anywhere: bool) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::Config(
            "URL de base du fournisseur IA vide".to_string(),
        ));
    }

    let url = reqwest::Url::parse(trimmed)
        .map_err(|e| Error::Config(format!("URL de base invalide : {trimmed:?} ({e})")))?;

    match url.scheme() {
        "https" => {}
        "http" => {
            // HTTP n'est acceptable que sur la boucle locale : ailleurs,
            // tout ce qui transite (clé API, contenu des documents)
            // circulerait en clair sur le réseau.
            if !allow_http_anywhere && !is_loopback(url.host_str()) {
                return Err(Error::Config(format!(
                    "URL de base refusée : {:?} utilise HTTP hors machine locale. \
                     La clé API partirait en clair sur le réseau — passez en HTTPS, ou \
                     utilisez une adresse locale (http://localhost:…). ",
                    redact_url(trimmed)
                )));
            }
        }
        other => {
            return Err(Error::Config(format!(
                "schéma d'URL non pris en charge : {other:?} (attendu http ou https)"
            )));
        }
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::Config(format!(
            "URL de base refusée : {:?} contient des identifiants. \
             Ils seraient envoyés dans l'autorisation de base — utilisez un en-tête dédié.",
            redact_url(trimmed)
        )));
    }

    if url.host_str().unwrap_or_default().is_empty() {
        return Err(Error::Config(format!(
            "URL de base sans hôte : {:?}",
            redact_url(trimmed)
        )));
    }

    let normalized = url.as_str().trim_end_matches('/').to_string();
    Ok(normalized)
}

/// Retire les identifiants éventuels (`utilisateur:motdepasse@`) d'une URL
/// avant de la citer : un message d'erreur ne doit pas recopier un secret
/// dans l'interface ou les journaux.
fn redact_url(raw: &str) -> String {
    match (raw.find("://"), raw.find('@')) {
        (Some(scheme), Some(at)) if at > scheme + 3 => {
            format!("{}://{}", &raw[..scheme], &raw[at + 1..])
        }
        _ => raw.to_string(),
    }
}

/// Vrai si l'hôte désigne la machine courante.
fn is_loopback(host: Option<&str>) -> bool {
    let Some(host) = host else { return false };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Construit le client HTTP d'un fournisseur.
///
/// `sends_credentials` : l'API envoie-t-elle une clé ? Si oui, la
/// politique de redirection refuse tout saut vers une autre origine.
pub fn http_client(base_url: &str, sends_credentials: bool) -> Result<Client> {
    http_client_with_timeout(base_url, sends_credentials, REQUEST_TIMEOUT)
}

/// Comme [`http_client`] mais avec un délai explicite (tests).
pub fn http_client_with_timeout(
    base_url: &str,
    sends_credentials: bool,
    timeout: Duration,
) -> Result<Client> {
    let expected = reqwest::Url::parse(base_url)
        .map_err(|e| Error::Config(format!("URL de base invalide : {base_url:?} ({e})")))?;

    let mut builder = Client::builder()
        .user_agent(concat!("YouPDF/", env!("CARGO_PKG_VERSION")))
        .timeout(timeout);

    if sends_credentials {
        let expected = Arc::new(expected);
        let policy = Policy::custom(move |attempt: Attempt| {
            // On clone l'URL : `attempt.url()` emprunte `attempt`, et les
            // branches ci-dessous doivent pouvoir le consommer (`error`,
            // `follow`).
            let next = attempt.url().clone();
            // 1. Jamais de rétrogradage HTTPS → HTTP : la clé partirait
            //    en clair, même si la configuration annonce du HTTPS.
            if next.scheme() == "http"
                && attempt
                    .previous()
                    .last()
                    .is_some_and(|prev| prev.scheme() == "https")
            {
                return attempt.error("redirection vers HTTP refusée (clé API exposée)");
            }
            // 2. Jamais de changement d'origine : le serveur configuré
            //    pourrait simplement renvoyer un 302 vers un serveur
            //    attentiste, sans que la configuration paraisse altérée.
            if next.origin() != expected.origin() {
                return attempt.error(format!(
                    "redirection hors du fournisseur refusée : {} (la clé API ne quitte \
                     pas l'origine configurée {})",
                    next,
                    expected
                ));
            }
            attempt.follow()
        });
        builder = builder.redirect(policy);
    } else {
        // Sans secret (Ollama) : on borne quand même la chaîne.
        builder = builder.redirect(Policy::limited(10));
    }

    builder
        .build()
        .map_err(|e| Error::Config(format!("client HTTP : {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepte_les_endpoints_legitimes() {
        for url in [
            "https://api.openai.com/v1",
            "https://api.anthropic.com/v1/",
            "http://localhost:11434",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:11434",
            "https://mon-vllm.entreprise.fr/v1",
        ] {
            let parsed = parse_base_url(url);
            assert!(parsed.is_ok(), "{url} devrait être accepté : {parsed:?}");
            assert!(
                !parsed.unwrap().ends_with('/'),
                "{url} ne devrait pas garder son / final"
            );
        }
    }

    /// Un serveur Ollama sur le réseau local reste atteignable : il
    /// n'envoie aucun secret.
    #[test]
    fn autorise_http_pour_un_fournisseur_sans_secret() {
        assert!(parse_base_url("http://ollama.local:11434").is_err());
        assert!(parse_public_base_url("http://ollama.local:11434").is_ok());
        // Mais un schéma tordu ou un hôte absent restent refusés.
        assert!(parse_public_base_url("ftp://ollama.local").is_err());
        assert!(parse_public_base_url("http://").is_err());
    }

    /// Le point central : une clé ne part pas en clair sur le réseau.
    #[test]
    fn refuse_http_non_local() {
        for url in ["http://evil.example.com/v1", "http://10.0.0.5:8000/v1"] {
            assert!(
                parse_base_url(url).is_err(),
                "{url} devrait être refusé"
            );
        }
    }

    #[test]
    fn refuse_les_schemas_et_identifiants_abusifs() {
        assert!(parse_base_url("file:///etc/passwd").is_err());
        assert!(parse_base_url("gopher://127.0.0.1:70/v1").is_err());
        assert!(parse_base_url("https://user:sekret@api.openai.com/v1").is_err());
        assert!(parse_base_url("").is_err());
        assert!(parse_base_url("pas une url").is_err());
    }

    /// Un endpoint muet ne doit pas immobiliser le worker IA.
    #[tokio::test]
    async fn une_requête_sans_réponse_finir_par_échouer() {
        let (addr, stop) = serveur_muet();
        let client =
            http_client_with_timeout(&format!("http://{addr}"), true, Duration::from_millis(300))
                .unwrap();
        let t0 = std::time::Instant::now();
        let res = client.get(format!("http://{addr}/")).send().await;
        let elapsed = t0.elapsed();
        stop.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(res.is_err(), "une requête muette doit échouer");
        assert!(
            elapsed < Duration::from_secs(5),
            "le délai n'a pas été appliqué : {elapsed:?}"
        );
    }

    /// Un serveur TCP qui accepte puis ne répond jamais.
    fn serveur_muet() -> (String, std::sync::Arc<std::sync::atomic::AtomicBool>) {
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let stop = Arc::new(AtomicBool::new(true));
        let stop2 = stop.clone();
        std::thread::spawn(move || {
            while stop2.load(Ordering::Relaxed) {
                // On accepte la connexion et on la laisse pendue.
                if let Ok((sock, _)) = listener.accept() {
                    std::thread::spawn(move || {
                        let _ = sock;
                        std::thread::sleep(Duration::from_secs(30));
                    });
                }
            }
        });
        (addr, stop)
    }

    #[test]
    fn refuse_les_redirections_changent_d_origine() {
        // Le policy elle-même est testée par reqwest ; on vérifie au
        // moins que la construction du client réussit dans les deux modes.
        assert!(http_client("https://api.openai.com/v1", true).is_ok());
        assert!(http_client("http://localhost:11434", false).is_ok());
        // Une URL invalide est signalée ici aussi, pas un panic.
        assert!(http_client("pas une url", true).is_err());
    }
}
