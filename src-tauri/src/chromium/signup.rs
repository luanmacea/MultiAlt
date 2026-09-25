//! Criação de contas em série no formulário de cadastro do Roblox.
//!
//! O app preenche tudo (usuário, senha, data de nascimento, gênero) e o
//! **humano resolve o CAPTCHA** e confirma. Nada aqui tenta resolver, burlar ou
//! esconder a verificação do Roblox — o objetivo é só não ter que digitar 16
//! cadastros na mão.
//!
//! Este arquivo é a metade **pura**: gerar a identidade e montar o script de
//! preenchimento. Quem abre o browser e espera o cookie é
//! `commands.rs::start_signup_session`.

use serde::{Deserialize, Serialize};

/// Uma identidade de cadastro pronta para o formulário.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignupIdentity {
    pub username: String,
    pub password: String,
    /// Dia com dois dígitos, como o `<select>` espera (`"07"`).
    pub day: String,
    /// Mês em três letras em inglês, que é o `value` do `<select>` (`"Mar"`).
    pub month: String,
    pub year: String,
    /// Sempre `"male"` hoje; o campo existe para a UI mostrar o que foi usado.
    pub gender: String,
}

impl SignupIdentity {
    /// Data no formato que a UI mostra.
    pub fn birthday(&self) -> String {
        format!("{}/{}/{}", self.day, self.month, self.year)
    }
}

/// Idade mínima da conta gerada. Contas abaixo de 13 entram no modo infantil do
/// Roblox (chat e experiências limitados), que não serve para nada aqui; 18+ é
/// o que o usuário pediu.
pub const MIN_AGE: i32 = 18;
/// Idade máxima, só para as datas não ficarem todas iguais.
pub const MAX_AGE: i32 = 40;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Pedaços de nome de usuário. Palavras comuns em inglês, sem nada que pareça
/// nome real nem ofensa — o Roblox recusa no cadastro e queimaria a tentativa.
const NAME_HEADS: [&str; 24] = [
    "Swift", "Brave", "Lucky", "Rapid", "Silent", "Cosmic", "Turbo", "Mega", "Nova", "Solar",
    "Frost", "Shadow", "Thunder", "Crimson", "Golden", "Iron", "Stone", "Storm", "Blaze", "Neon",
    "Hyper", "Ultra", "Prime", "Epic",
];

const NAME_TAILS: [&str; 24] = [
    "Falcon", "Tiger", "Wolf", "Raven", "Rider", "Runner", "Hunter", "Pilot", "Ranger", "Knight",
    "Dragon", "Comet", "Rocket", "Panda", "Otter", "Badger", "Viper", "Hawk", "Bear", "Lynx",
    "Puma", "Eagle", "Shark", "Bison",
];

/// Caracteres da senha. Sem `0`/`O`/`l`/`1` e sem símbolos: a senha é mostrada
/// na UI para o usuário guardar, e ambiguidade aí vira conta perdida.
const PASSWORD_ALPHABET: [u8; 55] = *b"ABCDEFGHJKMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";

/// Gerador pseudoaleatório determinístico (xorshift64*).
///
/// O projeto não tem dependência de RNG, e um gerador com semente explícita é
/// o que torna a geração testável: a mesma semente sempre dá a mesma conta.
#[derive(Debug, Clone)]
pub struct SeededRng(u64);

impl SeededRng {
    pub fn new(seed: u64) -> Self {
        // Semente 0 travaria o xorshift em zero para sempre.
        Self(if seed == 0 { 0x9E3779B97F4A7C15 } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Inteiro em `[0, bound)`. `bound == 0` devolve 0 em vez de dividir por zero.
    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() % bound as u64) as usize
    }
}

/// Nome de usuário válido para o Roblox: 3–20 caracteres, letras e dígitos com
/// no máximo **um** underscore, que não pode ficar na ponta.
///
/// O formato é `Cabeça_Cauda##`; se passar de 20 caracteres, o sufixo numérico
/// é cortado antes das palavras, para o nome continuar legível.
pub fn generate_username(rng: &mut SeededRng) -> String {
    let head = NAME_HEADS[rng.below(NAME_HEADS.len())];
    let tail = NAME_TAILS[rng.below(NAME_TAILS.len())];
    let digits = 10 + rng.below(990); // 2 a 3 dígitos, nunca começando em 0

    let mut name = format!("{}_{}{}", head, tail, digits);
    if name.len() > 20 {
        name = format!("{}_{}", head, tail);
        name.truncate(20);
    }
    // Um corte não pode deixar o underscore na ponta.
    while name.ends_with('_') {
        name.pop();
    }
    name
}

/// Senha de 14 caracteres com pelo menos uma letra e um dígito (o Roblox exige
/// mais de 8 caracteres e recusa senha que contenha o nome de usuário).
pub fn generate_password(rng: &mut SeededRng) -> String {
    let mut password = String::with_capacity(14);
    for _ in 0..14 {
        let index = rng.below(PASSWORD_ALPHABET.len());
        password.push(PASSWORD_ALPHABET[index] as char);
    }
    // Garante a mistura mesmo num sorteio infeliz.
    password.replace_range(0..1, "R");
    password.replace_range(13..14, "7");
    password
}

/// Data de nascimento entre [`MIN_AGE`] e [`MAX_AGE`] anos.
///
/// `current_year` entra como parâmetro (e não do relógio) para o teste poder
/// fixar o ano. O dia fica em 1–28 de propósito: assim nenhuma combinação de
/// mês cai num dia inexistente.
pub fn generate_birthday(rng: &mut SeededRng, current_year: i32) -> (String, String, String) {
    let age = MIN_AGE + rng.below((MAX_AGE - MIN_AGE + 1) as usize) as i32;
    let year = current_year - age;
    let month = MONTHS[rng.below(MONTHS.len())];
    let day = 1 + rng.below(28);
    (format!("{:02}", day), month.to_string(), year.to_string())
}

/// Uma identidade completa a partir de uma semente.
pub fn generate_identity(seed: u64, current_year: i32) -> SignupIdentity {
    let mut rng = SeededRng::new(seed);
    let username = generate_username(&mut rng);
    let password = generate_password(&mut rng);
    let (day, month, year) = generate_birthday(&mut rng, current_year);
    SignupIdentity {
        username,
        password,
        day,
        month,
        year,
        gender: "male".to_string(),
    }
}

/// Script que preenche o formulário de cadastro.
///
/// Notas de implementação que já custaram caro:
///
/// - os campos são controlados pelo React, então escrever `.value` direto não
///   atualiza o estado: é preciso usar o setter nativo do prototype e disparar
///   o evento (mesma técnica do `login_fill_script`);
/// - os `<select>` de data **não têm id**; eles são achados pelo
///   `data-testid` do container (`birthday-day`/`birthday-month`/`birthday-year`),
///   com os ids antigos (`#DayDropdown`...) como reserva;
/// - o botão de gênero é achado pelo ícone (`icon-regular-head-male`), que não
///   muda com o idioma da página — o texto ("Masculino"/"Male") muda;
/// - **nada aqui clica em "Criar conta"**. O envio é do usuário, depois do
///   CAPTCHA.
pub fn signup_fill_script(identity: &SignupIdentity) -> String {
    let user = serde_json::to_string(&identity.username).unwrap_or_default();
    let pass = serde_json::to_string(&identity.password).unwrap_or_default();
    let day = serde_json::to_string(&identity.day).unwrap_or_default();
    let month = serde_json::to_string(&identity.month).unwrap_or_default();
    let year = serde_json::to_string(&identity.year).unwrap_or_default();

    format!(
        "(function(){{\
var setText=function(el,v){{if(!el)return false;\
var s=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set;\
s.call(el,v);el.dispatchEvent(new Event('input',{{bubbles:true}}));\
el.dispatchEvent(new Event('change',{{bubbles:true}}));return true;}};\
var setSelect=function(el,v){{if(!el)return false;\
var s=Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype,'value').set;\
s.call(el,v);el.dispatchEvent(new Event('change',{{bubbles:true}}));return el.value===v;}};\
var pick=function(testid,legacy){{\
return document.querySelector('[data-testid=\"'+testid+'\"] select')||document.querySelector(legacy);}};\
var done={{}};\
done.day=setSelect(pick('birthday-day','#DayDropdown'),{day});\
done.month=setSelect(pick('birthday-month','#MonthDropdown'),{month});\
done.year=setSelect(pick('birthday-year','#YearDropdown'),{year});\
done.username=setText(document.querySelector('#signup-username'),{user});\
done.password=setText(document.querySelector('#signup-password'),{pass});\
var male=Array.prototype.find.call(document.querySelectorAll('button'),function(b){{\
return b.querySelector('.icon-regular-head-male');}})||document.querySelector('#MaleButton');\
if(male){{male.click();done.gender=true;}}else{{done.gender=false;}}\
return JSON.stringify(done);}})()",
        day = day,
        month = month,
        year = year,
        user = user,
        pass = pass
    )
}

/// O formulário de cadastro está na tela?
pub const SIGNUP_FORM_SELECTOR: &str = "#signup-username";

/// Página de cadastro. Sem prefixo de idioma: o Roblox redireciona para o
/// idioma do navegador sozinho, e os seletores usados não dependem disso.
pub const ROBLOX_SIGNUP_URL: &str = "https://www.roblox.com/CreateAccount";

#[cfg(test)]
mod signup_identity_tests {
    use super::*;

    /// Regras do Roblox para nome de usuário: 3–20 caracteres, só letras,
    /// dígitos e no máximo um underscore, nunca nas pontas.
    fn assert_valid_username(name: &str) {
        assert!(name.len() >= 3 && name.len() <= 20, "tamanho inválido: {}", name);
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "caractere inválido: {}",
            name
        );
        assert_eq!(name.matches('_').count(), 1, "underscores demais: {}", name);
        assert!(!name.starts_with('_') && !name.ends_with('_'), "underscore na ponta: {}", name);
    }

    #[test]
    fn generated_usernames_always_follow_the_roblox_rules() {
        for seed in 0..500u64 {
            assert_valid_username(&generate_username(&mut SeededRng::new(seed)));
        }
    }

    #[test]
    fn generated_passwords_are_long_enough_and_mixed() {
        for seed in 0..200u64 {
            let password = generate_password(&mut SeededRng::new(seed));
            assert!(password.len() >= 8, "curta demais: {}", password);
            assert!(password.chars().any(|c| c.is_ascii_alphabetic()));
            assert!(password.chars().any(|c| c.is_ascii_digit()));
            // Sem caracteres ambíguos: a senha é lida da tela pelo usuário.
            assert!(!password.contains(['0', 'O', 'l', '1']), "ambígua: {}", password);
        }
    }

    #[test]
    fn a_generated_account_is_never_under_eighteen() {
        for seed in 0..300u64 {
            let (day, month, year) = generate_birthday(&mut SeededRng::new(seed), 2026);
            let year: i32 = year.parse().expect("ano numérico");
            let age = 2026 - year;
            assert!((MIN_AGE..=MAX_AGE).contains(&age), "idade {} fora da faixa", age);
            // Dia 1–28 evita 30 de fevereiro em qualquer mês sorteado.
            let day: i32 = day.parse().expect("dia numérico");
            assert!((1..=28).contains(&day), "dia inválido: {}", day);
            assert!(MONTHS.contains(&month.as_str()), "mês inválido: {}", month);
        }
    }

    #[test]
    fn the_day_keeps_the_two_digit_form_the_select_expects() {
        // O `<select>` do Roblox tem `value="01"`, não `value="1"`.
        for seed in 0..100u64 {
            let (day, _, _) = generate_birthday(&mut SeededRng::new(seed), 2026);
            assert_eq!(day.len(), 2, "dia sem dois dígitos: {}", day);
        }
    }

    #[test]
    fn the_same_seed_always_produces_the_same_identity() {
        assert_eq!(generate_identity(4242, 2026), generate_identity(4242, 2026));
    }

    #[test]
    fn different_seeds_produce_different_accounts() {
        let names: std::collections::HashSet<String> = (0..200u64)
            .map(|seed| generate_identity(seed, 2026).username)
            .collect();
        // Colisão ocasional é aceitável (o Roblox recusa e o usuário tenta de
        // novo); um gerador que repete quase tudo não é.
        assert!(names.len() > 150, "só {} nomes distintos em 200", names.len());
    }

    #[test]
    fn a_zero_seed_does_not_freeze_the_generator() {
        let mut rng = SeededRng::new(0);
        let first = rng.below(1000);
        let second = rng.below(1000);
        assert_ne!(first, second, "xorshift travou em zero");
        assert_eq!(rng.below(0), 0, "bound zero não pode dividir por zero");
    }

    #[test]
    fn the_identity_exposes_a_readable_birthday() {
        let identity = generate_identity(7, 2026);
        assert_eq!(
            identity.birthday(),
            format!("{}/{}/{}", identity.day, identity.month, identity.year)
        );
        assert_eq!(identity.gender, "male");
    }
}

#[cfg(test)]
mod signup_script_tests {
    use super::*;

    fn script_for(username: &str, password: &str) -> String {
        signup_fill_script(&SignupIdentity {
            username: username.to_string(),
            password: password.to_string(),
            day: "07".to_string(),
            month: "Mar".to_string(),
            year: "2001".to_string(),
            gender: "male".to_string(),
        })
    }

    #[test]
    fn the_script_targets_the_current_signup_form() {
        let script = script_for("Swift_Falcon12", "Rabcdefghij7");
        assert!(script.contains("#signup-username"));
        assert!(script.contains("#signup-password"));
        assert!(script.contains("birthday-day"));
        assert!(script.contains("birthday-month"));
        assert!(script.contains("birthday-year"));
        assert!(script.contains("icon-regular-head-male"));
    }

    /// Os `<select>` de data não têm id na página nova; os ids antigos ficam
    /// como reserva para quando o Roblox voltar atrás (já aconteceu).
    #[test]
    fn the_legacy_dropdown_ids_are_kept_as_a_fallback() {
        let script = script_for("a_b12", "Rabcdefghij7");
        assert!(script.contains("#DayDropdown"));
        assert!(script.contains("#MonthDropdown"));
        assert!(script.contains("#YearDropdown"));
        assert!(script.contains("#MaleButton"));
    }

    /// Regra de produto: o envio é do usuário, depois do CAPTCHA. O script não
    /// pode clicar em "Criar conta".
    #[test]
    fn the_script_never_submits_the_form() {
        let script = script_for("Swift_Falcon12", "Rabcdefghij7");
        assert!(!script.contains("type=\\\"submit\\\""), "script: {}", script);
        assert!(!script.contains(".submit()"), "script: {}", script);
        // O único clique é o do botão de gênero.
        assert_eq!(script.matches(".click()").count(), 1);
    }

    /// Os valores gerados entram como literais JSON: nenhum deles pode fechar
    /// a string e virar código.
    #[test]
    fn values_are_embedded_as_json_literals() {
        for (user, pass) in [
            ("bob\"); alert(1); //", "p\\\"; evil()"),
            ("a\nb", "c\td"),
            ("<script>", "</script>"),
            ("", ""),
        ] {
            let script = script_for(user, pass);
            // Cada valor entra exatamente como o literal JSON dele: aspas
            // escapadas, nada de quebra de linha crua fechando a string.
            assert!(
                script.contains(&serde_json::to_string(user).unwrap()),
                "usuário {:?} não virou literal JSON",
                user
            );
            assert!(
                script.contains(&serde_json::to_string(pass).unwrap()),
                "senha {:?} não virou literal JSON",
                pass
            );
            assert!(!script.contains('\n'), "newline cru para {:?}", user);
            assert!(!script.contains('\t'), "tab cru para {:?}", pass);
        }
    }

    #[test]
    fn the_script_is_stable_for_the_same_identity() {
        assert_eq!(script_for("a_b12", "Rabc7"), script_for("a_b12", "Rabc7"));
    }

    #[test]
    fn the_birthday_values_are_the_select_values_not_the_labels() {
        let script = script_for("a_b12", "Rabc7");
        assert!(script.contains("\"Mar\""), "mês tem que ser o value do select");
        assert!(script.contains("\"07\""));
        assert!(script.contains("\"2001\""));
    }
}
