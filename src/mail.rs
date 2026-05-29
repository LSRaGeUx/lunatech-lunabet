use anyhow::Context;
use askama::Template;
use lettre::message::{Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::Config;
use crate::i18n::Locale;
use crate::tenant::Tenant;

#[derive(Template)]
#[template(path = "emails/magic_link.html")]
struct MagicLinkHtml<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    link: &'a str,
    logo_url: &'a str,
}

#[derive(Template)]
#[template(path = "emails/signup_verification.html")]
struct SignupVerificationHtml<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    new_tenant_name: &'a str,
    owner_name: &'a str,
    link: &'a str,
    logo_url: &'a str,
}

#[derive(Template)]
#[template(path = "emails/match_reminder.html")]
struct MatchReminderHtml<'a> {
    tenant: &'a Tenant,
    home: &'a str,
    away: &'a str,
    kickoff_local: &'a str,
    matches_url: &'a str,
    logo_url: &'a str,
}

pub async fn send_magic_link(
    cfg: &Config,
    tenant: &Tenant,
    loc: Locale,
    base_url: &str,
    to: &str,
    link: &str,
) -> anyhow::Result<()> {
    let logo_url = match tenant.logo_url.as_deref() {
        Some(u) if u.starts_with("http") => u.to_string(),
        Some(rel) => format!("{}{}", base_url.trim_end_matches('/'), rel),
        None => format!("{}/static/lunatech-logo.svg", base_url.trim_end_matches('/')),
    };
    let html = MagicLinkHtml { loc, tenant, link, logo_url: &logo_url }.render()?;

    let plain = format!(
        "{salut}\n\n\
         {intro} {link}\n\n\
         {expire}\n\n\
         {ignore}\n\n\
         - {brand} · LunaBet\n",
        salut = loc.f("Salut !", "Hi!"),
        intro = loc.f(
            "Voici ton lien de connexion :",
            "Here is your sign-in link:"
        ),
        expire = loc.f(
            "Il est valable 15 minutes.",
            "It is valid for 15 minutes."
        ),
        ignore = loc.f(
            "Si tu n'as pas demandé ce lien, ignore simplement cet email.",
            "If you didn't request this link, just ignore this email."
        ),
        brand = tenant.name,
    );

    let subject = match loc {
        Locale::Fr => format!("Ton lien de connexion {}", tenant.name),
        Locale::En => format!("Your {} sign-in link", tenant.name),
    };
    send_html_email(cfg, &tenant.mail_from, to, &subject, plain, html).await
}

pub async fn send_bet_reminder(
    cfg: &Config,
    tenant: &Tenant,
    to: &str,
    home: &str,
    away: &str,
    kickoff_local: &str,
    base_url: &str,
) -> anyhow::Result<()> {
    let matches_url = format!("{}/matches", base_url.trim_end_matches('/'));
    let logo_url = match tenant.logo_url.as_deref() {
        Some(u) if u.starts_with("http") => u.to_string(),
        Some(rel) => format!("{}{}", base_url.trim_end_matches('/'), rel),
        None => format!("{}/static/lunatech-logo.svg", base_url.trim_end_matches('/')),
    };
    let html = MatchReminderHtml {
        tenant,
        home,
        away,
        kickoff_local,
        matches_url: &matches_url,
        logo_url: &logo_url,
    }
    .render()?;

    let plain = format!(
        "Salut ! / Hi!\n\n\
         FR - {home} - {away} commence bientôt ({kickoff_local}) et tu n'as pas encore parié.\n\
         EN - {home} - {away} kicks off soon ({kickoff_local}) and you haven't bet yet.\n\n\
         FR - Va placer ton pronostic : {matches_url}\n\
         EN - Place your prediction: {matches_url}\n\n\
         Bonne chance ! / Good luck!\n\n\
         - {brand} · LunaBet\n",
        brand = tenant.name,
    );

    let subject = format!("⚽ {home} - {away} - {}", tenant.name);
    send_html_email(cfg, &tenant.mail_from, to, &subject, plain, html).await
}

/// Magic-link email for the platform-level admin login (`/super-admin/`).
/// Plain text only — the dashboard is internal, no need for the branded
/// HTML template the tenant login uses.
pub async fn send_platform_magic_link(
    cfg: &Config,
    loc: Locale,
    base_url: &str,
    to: &str,
    link: &str,
) -> anyhow::Result<()> {
    let plain = match loc {
        Locale::Fr => format!(
            "Salut,\n\n\
             Voici ton lien de connexion super-admin LunaBet.\n\
             Il est valable 15 minutes :\n\n\
             {link}\n\n\
             Si ce n'est pas toi, ignore cet email.\n\n\
             - LunaBet platform\n"
        ),
        Locale::En => format!(
            "Hi,\n\n\
             Here is your LunaBet super-admin sign-in link.\n\
             It is valid for 15 minutes:\n\n\
             {link}\n\n\
             If this wasn't you, just ignore this email.\n\n\
             - LunaBet platform\n"
        ),
    };
    let html = format!(
        "<p>{}</p><p><a href=\"{link}\">{link}</a></p><p style=\"color:#888;font-size:0.9em;\">{}</p>",
        match loc {
            Locale::Fr => "Ton lien de connexion super-admin LunaBet (15 min) :",
            Locale::En => "Your LunaBet super-admin sign-in link (15 min):",
        },
        match loc {
            Locale::Fr => "Si ce n'est pas toi, ignore cet email.",
            Locale::En => "If this wasn't you, just ignore this email.",
        },
    );

    let _ = base_url; // base_url reserved for future use (footer links)
    let subject = match loc {
        Locale::Fr => "LunaBet · Lien super-admin",
        Locale::En => "LunaBet · super-admin sign-in",
    };
    // Use a platform-neutral From: derived from the SMTP MAIL_FROM (which
    // belongs to the platform operator), not a per-tenant address.
    send_html_email(cfg, &cfg.mail_from, to, subject, plain, html).await
}

pub async fn send_signup_verification(
    cfg: &Config,
    tenant: &Tenant,
    loc: Locale,
    base_url: &str,
    to: &str,
    owner_name: &str,
    new_tenant_name: &str,
    link: &str,
) -> anyhow::Result<()> {
    let logo_url = format!("{}/static/lunatech-logo.svg", base_url.trim_end_matches('/'));
    let html = SignupVerificationHtml {
        loc,
        tenant,
        new_tenant_name,
        owner_name,
        link,
        logo_url: &logo_url,
    }
    .render()?;

    let plain = match loc {
        Locale::Fr => format!(
            "Salut {owner_name} !\n\n\
             Tu as demandé à créer un espace LunaBet pour « {new_tenant_name} ». \
             Clique sur ce lien pour confirmer (valable 30 minutes) :\n\n\
             {link}\n\n\
             Si ce n'est pas toi, ignore simplement cet email.\n\n\
             - LunaBet\n"
        ),
        Locale::En => format!(
            "Hi {owner_name}!\n\n\
             You requested a LunaBet space for \"{new_tenant_name}\". \
             Click this link to confirm (valid for 30 minutes):\n\n\
             {link}\n\n\
             If this wasn't you, just ignore this email.\n\n\
             - LunaBet\n"
        ),
    };

    let subject = match loc {
        Locale::Fr => format!("Confirme la création de {new_tenant_name} sur LunaBet"),
        Locale::En => format!("Confirm your LunaBet space for {new_tenant_name}"),
    };
    send_html_email(cfg, &tenant.mail_from, to, &subject, plain, html).await
}

async fn send_html_email(
    cfg: &Config,
    mail_from: &str,
    to: &str,
    subject: &str,
    plain: String,
    html: String,
) -> anyhow::Result<()> {
    let from: Mailbox = mail_from.parse().context("tenant mail_from is invalid")?;
    let to_addr: Mailbox = to.parse().context("recipient email is invalid")?;

    let email = Message::builder()
        .from(from)
        .to(to_addr)
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(SinglePart::plain(plain))
                .singlepart(SinglePart::html(html)),
        )?;

    let mut builder = if cfg.smtp_starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.smtp_host)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.smtp_host)
    };
    builder = builder.port(cfg.smtp_port);
    if let (Some(u), Some(p)) = (&cfg.smtp_username, &cfg.smtp_password) {
        builder = builder.credentials(Credentials::new(u.clone(), p.clone()));
    }
    let mailer = builder.build();

    mailer.send(email).await.context("sending email")?;
    Ok(())
}
