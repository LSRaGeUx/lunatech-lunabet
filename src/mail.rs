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
    new_tenant_name: &'a str,
    owner_name: &'a str,
    link: &'a str,
    logo_url: &'a str,
}

#[derive(Template)]
#[template(path = "emails/match_reminder.html")]
struct MatchReminderHtml<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    home: &'a str,
    away: &'a str,
    kickoff_local: &'a str,
    matches_url: &'a str,
    logo_url: &'a str,
}

#[derive(Template)]
#[template(path = "emails/invitation.html")]
struct InvitationHtml<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    inviter_name: &'a str,
    link: &'a str,
    logo_url: &'a str,
}

/// Invitation email: someone invites `to` to join `tenant`. The link both
/// joins the space and signs the invitee in on first click.
pub async fn send_invitation(
    cfg: &Config,
    tenant: &Tenant,
    loc: Locale,
    base_url: &str,
    to: &str,
    inviter_name: &str,
    link: &str,
) -> anyhow::Result<()> {
    let logo_url = match tenant.logo_url.as_deref() {
        Some(u) if u.starts_with("http") => u.to_string(),
        Some(rel) => format!("{}{}", base_url.trim_end_matches('/'), rel),
        None => format!("{}/static/lunatech-logo.svg", base_url.trim_end_matches('/')),
    };
    let html = InvitationHtml { loc, tenant, inviter_name, link, logo_url: &logo_url }.render()?;

    let plain = format!(
        "{salut}\n\n\
         {intro}\n\n\
         {join} {link}\n\n\
         {expire}\n\n\
         - {brand} · LunaBet\n",
        salut = loc.f("Salut !", "Hi!"),
        intro = match loc {
            Locale::Fr => format!("{inviter_name} t'invite à rejoindre {} sur LunaBet.", tenant.name),
            Locale::En => format!("{inviter_name} invited you to join {} on LunaBet.", tenant.name),
        },
        join = loc.f("Rejoins l'espace :", "Join the space:"),
        expire = loc.f(
            "Ce lien est valable 7 jours.",
            "This link is valid for 7 days."
        ),
        brand = tenant.name,
    );

    let subject = match loc {
        Locale::Fr => format!("{inviter_name} t'invite sur {}", tenant.name),
        Locale::En => format!("{inviter_name} invited you to {}", tenant.name),
    };
    send_html_email(cfg, &cfg.mail_from, to, &subject, plain, html).await
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
    // Always send from the platform's MAIL_FROM: it's the only address our
    // SMTP relay is verified for. The tenant name still appears in the
    // subject and body for branding.
    send_html_email(cfg, &cfg.mail_from, to, &subject, plain, html).await
}

pub async fn send_bet_reminder(
    cfg: &Config,
    tenant: &Tenant,
    loc: Locale,
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
        loc,
        tenant,
        home,
        away,
        kickoff_local,
        matches_url: &matches_url,
        logo_url: &logo_url,
    }
    .render()?;

    let line1 = match loc {
        Locale::Fr => format!("{home} - {away} commence bientôt ({kickoff_local}) et tu n'as pas encore parié."),
        Locale::En => format!("{home} - {away} kicks off soon ({kickoff_local}) and you haven't bet yet."),
    };
    let line2 = loc.f("Va placer ton pronostic :", "Place your prediction:");
    let plain = format!(
        "{hi}\n\n\
         {line1}\n\n\
         {line2} {matches_url}\n\n\
         {luck}\n\n\
         - {brand} · LunaBet\n",
        hi = loc.f("Salut !", "Hi!"),
        luck = loc.f("Bonne chance !", "Good luck!"),
        brand = tenant.name,
    );

    let subject = format!("⚽ {home} - {away} - {}", tenant.name);
    send_html_email(cfg, &cfg.mail_from, to, &subject, plain, html).await
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
    loc: Locale,
    base_url: &str,
    to: &str,
    owner_name: &str,
    new_tenant_name: &str,
    link: &str,
) -> anyhow::Result<()> {
    // Signup is platform-level: the new tenant doesn't exist yet, so we
    // can't use its mail_from. Always send from the platform's MAIL_FROM,
    // which the relay knows about.
    let from = &cfg.mail_from;
    let logo_url = format!("{}/static/lunatech-logo.svg", base_url.trim_end_matches('/'));
    let html = SignupVerificationHtml {
        loc,
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
    send_html_email(cfg, from, to, &subject, plain, html).await
}

/// A score the user predicted (home, away).
pub struct ScorePair {
    pub home: i32,
    pub away: i32,
}

/// One of today's matches, with the recipient's current prediction (if any).
pub struct TodayMatch {
    pub home: String,
    pub away: String,
    pub kickoff_local: String,
    pub bet: Option<ScorePair>,
}

#[derive(Template)]
#[template(path = "emails/today_matches.html")]
struct TodayMatchesHtml<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    logo_url: &'a str,
    day_label: &'a str,
    matches: &'a [TodayMatch],
    matches_url: &'a str,
}

/// Morning preview of the day's matches: lists every match kicking off today
/// and, for each, the recipient's current prediction with a nudge that there
/// is still time to change it. Localised per recipient.
pub async fn send_today_matches_email(
    cfg: &Config,
    tenant: &Tenant,
    loc: Locale,
    to: &str,
    day_label: &str,
    matches: &[TodayMatch],
    base_url: &str,
) -> anyhow::Result<()> {
    let matches_url = format!("{}/matches", base_url.trim_end_matches('/'));
    let logo_url = match tenant.logo_url.as_deref() {
        Some(u) if u.starts_with("http") => u.to_string(),
        Some(rel) => format!("{}{}", base_url.trim_end_matches('/'), rel),
        None => format!("{}/static/lunatech-logo.svg", base_url.trim_end_matches('/')),
    };
    let html = TodayMatchesHtml {
        loc,
        tenant,
        logo_url: &logo_url,
        day_label,
        matches,
        matches_url: &matches_url,
    }
    .render()?;

    let mut body = String::new();
    for m in matches {
        let when = &m.kickoff_local;
        match &m.bet {
            Some(b) => body.push_str(&format!(
                "  {} - {} ({when}) : {} {}-{} {}\n",
                m.home,
                m.away,
                loc.f("ton prono", "your prediction"),
                b.home,
                b.away,
                loc.f("(encore temps de changer)", "(still time to change)"),
            )),
            None => body.push_str(&format!(
                "  {} - {} ({when}) : {}\n",
                m.home,
                m.away,
                loc.f("pas encore de prono", "no prediction yet"),
            )),
        }
    }
    let plain = format!(
        "{hi}\n\n\
         {intro}\n{body}\n\
         {nudge}\n{matches_url}\n\n\
         - {brand} · LunaBet\n",
        hi = loc.f("Salut !", "Hi!"),
        intro = loc.f("Les matchs du jour :", "Today's matches:"),
        nudge = loc.f(
            "Il est encore temps de changer tes pronos :",
            "There is still time to change your predictions:"
        ),
        brand = tenant.name,
    );

    let subject = match loc {
        Locale::Fr => format!("⚽ Les matchs du {day_label} - {}", tenant.name),
        Locale::En => format!("⚽ {day_label} matches - {}", tenant.name),
    };
    send_html_email(cfg, &cfg.mail_from, to, &subject, plain, html).await
}

/// One finished match in the daily recap.
pub struct DigestResult {
    pub home: String,
    pub away: String,
    pub home_score: i32,
    pub away_score: i32,
    pub group: Option<String>,
}

/// One leaderboard line in the daily recap.
pub struct DigestStanding {
    pub rank: usize,
    pub name: String,
    pub points: i64,
    pub is_me: bool,
}

/// Best predictor of the recap day, highlighted at the top of the email.
pub struct DigestPotd {
    pub name: String,
    pub points: i64,
}

#[derive(Template)]
#[template(path = "emails/daily_digest.html")]
struct DailyDigestHtml<'a> {
    loc: Locale,
    tenant: &'a Tenant,
    logo_url: &'a str,
    day_label: &'a str,
    results: &'a [DigestResult],
    potd: Option<&'a DigestPotd>,
    my_points: i64,
    my_rank: usize,
    my_total: i64,
    standings: &'a [DigestStanding],
    leaderboard_url: &'a str,
}

/// Daily recap email: the day's results, the points this user earned that day,
/// and the current leaderboard. Localised per recipient.
#[allow(clippy::too_many_arguments)]
pub async fn send_daily_digest_email(
    cfg: &Config,
    tenant: &Tenant,
    loc: Locale,
    to: &str,
    day_label: &str,
    results: &[DigestResult],
    potd: Option<&DigestPotd>,
    my_points: i64,
    my_rank: usize,
    my_total: i64,
    standings: &[DigestStanding],
    base_url: &str,
) -> anyhow::Result<()> {
    let leaderboard_url = format!("{}/leaderboard", base_url.trim_end_matches('/'));
    let logo_url = match tenant.logo_url.as_deref() {
        Some(u) if u.starts_with("http") => u.to_string(),
        Some(rel) => format!("{}{}", base_url.trim_end_matches('/'), rel),
        None => format!("{}/static/lunatech-logo.svg", base_url.trim_end_matches('/')),
    };
    let html = DailyDigestHtml {
        loc,
        tenant,
        logo_url: &logo_url,
        day_label,
        results,
        potd,
        my_points,
        my_rank,
        my_total,
        standings,
        leaderboard_url: &leaderboard_url,
    }
    .render()?;

    let mut results_txt = String::new();
    for r in results {
        let g = r.group.as_deref().map(|g| format!(" [{g}]")).unwrap_or_default();
        results_txt.push_str(&format!(
            "  {} {}-{} {}{}\n",
            r.home, r.home_score, r.away_score, r.away, g
        ));
    }
    let mut board_txt = String::new();
    for s in standings {
        let me = if s.is_me { "  <--" } else { "" };
        board_txt.push_str(&format!("  {}. {} - {} pts{}\n", s.rank, s.name, s.points, me));
    }
    let potd_line = match potd {
        Some(p) => match loc {
            Locale::Fr => format!("\n🏆 Joueur du jour : {} ({} pts)\n", p.name, p.points),
            Locale::En => format!("\n🏆 Player of the day: {} ({} pts)\n", p.name, p.points),
        },
        None => String::new(),
    };
    let plain = format!(
        "{hi}\n\n\
         {res_h}\n{results_txt}\
         {potd_line}\n\
         {pts_line}\n\n\
         {rank_line}\n\n\
         {board_h}\n{board_txt}\n\
         {board_link} {leaderboard_url}\n\n\
         - {brand} · LunaBet\n",
        hi = loc.f("Salut !", "Hi!"),
        res_h = loc.f("Résultats du", "Results for") .to_string() + " " + day_label + " :",
        pts_line = match loc {
            Locale::Fr => format!("Tu as marqué {my_points} pts ce jour-là."),
            Locale::En => format!("You scored {my_points} pts that day."),
        },
        rank_line = match loc {
            Locale::Fr => format!("Classement : tu es {my_rank}e avec {my_total} pts au total."),
            Locale::En => format!("Leaderboard: you are #{my_rank} with {my_total} pts total."),
        },
        board_h = loc.f("Classement du jour :", "Today's leaderboard:"),
        board_link = loc.f("Voir le classement complet :", "See the full leaderboard:"),
        brand = tenant.name,
    );

    let subject = match loc {
        Locale::Fr => format!("📊 Récap LunaBet du {day_label} - {}", tenant.name),
        Locale::En => format!("📊 LunaBet recap for {day_label} - {}", tenant.name),
    };
    send_html_email(cfg, &cfg.mail_from, to, &subject, plain, html).await
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
