use anyhow::Context;
use askama::Template;
use lettre::message::{Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::Config;
use crate::i18n::Locale;

#[derive(Template)]
#[template(path = "emails/magic_link.html")]
struct MagicLinkHtml<'a> {
    loc: Locale,
    link: &'a str,
    logo_url: &'a str,
}

#[derive(Template)]
#[template(path = "emails/match_reminder.html")]
struct MatchReminderHtml<'a> {
    home: &'a str,
    away: &'a str,
    kickoff_local: &'a str,
    matches_url: &'a str,
    logo_url: &'a str,
}

pub async fn send_magic_link(cfg: &Config, loc: Locale, to: &str, link: &str) -> anyhow::Result<()> {
    let logo_url = format!("{}/static/lunatech-logo.svg", cfg.base_url.trim_end_matches('/'));
    let html = MagicLinkHtml { loc, link, logo_url: &logo_url }.render()?;

    let plain = format!(
        "{salut}\n\n\
         {intro} {link}\n\n\
         {expire}\n\n\
         {ignore}\n\n\
         — LunaBet · Lunatech\n",
        salut = loc.f("Salut !", "Hi!"),
        intro = loc.f(
            "Voici ton lien de connexion à LunaBet :",
            "Here is your LunaBet sign-in link:"
        ),
        expire = loc.f(
            "Il est valable 15 minutes.",
            "It is valid for 15 minutes."
        ),
        ignore = loc.f(
            "Si tu n'as pas demandé ce lien, ignore simplement cet email.",
            "If you didn't request this link, just ignore this email."
        ),
    );

    let subject = loc.f(
        "Ton lien de connexion LunaBet",
        "Your LunaBet sign-in link",
    );
    send_html_email(cfg, to, subject, plain, html).await
}

pub async fn send_bet_reminder(
    cfg: &Config,
    to: &str,
    home: &str,
    away: &str,
    kickoff_local: &str,
    base_url: &str,
) -> anyhow::Result<()> {
    let matches_url = format!("{}/matches", base_url.trim_end_matches('/'));
    let logo_url = format!("{}/static/lunatech-logo.svg", base_url.trim_end_matches('/'));
    let html = MatchReminderHtml {
        home,
        away,
        kickoff_local,
        matches_url: &matches_url,
        logo_url: &logo_url,
    }
    .render()?;

    let plain = format!(
        "Salut ! / Hi!\n\n\
         FR — {home} - {away} commence bientôt ({kickoff_local}) et tu n'as pas encore parié.\n\
         EN — {home} - {away} kicks off soon ({kickoff_local}) and you haven't bet yet.\n\n\
         FR — Va placer ton pronostic : {matches_url}\n\
         EN — Place your prediction: {matches_url}\n\n\
         Bonne chance ! / Good luck!\n\n\
         — LunaBet · Lunatech\n"
    );

    let subject = format!("⚽ {home} - {away} — LunaBet");
    send_html_email(cfg, to, &subject, plain, html).await
}

async fn send_html_email(
    cfg: &Config,
    to: &str,
    subject: &str,
    plain: String,
    html: String,
) -> anyhow::Result<()> {
    let from: Mailbox = cfg.mail_from.parse().context("MAIL_FROM is invalid")?;
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
