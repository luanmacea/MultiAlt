import { describe, expect, it } from "vitest";
import {
  accountInitial,
  accountLabel,
  hideAccountAvatar,
  maskAccountName,
  rawAccountLabel,
} from "./accountName";

const ANN = { Username: "annabelle", Alias: "" };
const ALIASED = { Username: "annabelle", Alias: "Main" };

describe("maskAccountName", () => {
  it("devolve o nome intacto com o modo desligado", () => {
    expect(maskAccountName("annabelle", false, 3)).toBe("annabelle");
  });

  it("mostra só as letras de prévia e esconde o resto", () => {
    expect(maskAccountName("annabelle", true, 3)).toBe("ann********");
  });

  it("sem letras de prévia, ou com prévia que mostraria tudo, esconde o nome inteiro", () => {
    expect(maskAccountName("annabelle", true, 0)).toBe("************");
    expect(maskAccountName("ann", true, 3)).toBe("************");
    expect(maskAccountName("ann", true, 9)).toBe("************");
  });
});

describe("rawAccountLabel", () => {
  it("prefere o alias e cai no username", () => {
    expect(rawAccountLabel(ALIASED)).toBe("Main");
    expect(rawAccountLabel(ANN)).toBe("annabelle");
  });

  it("sem conta usa o fallback (número vira texto)", () => {
    expect(rawAccountLabel(undefined, "User ID: 7")).toBe("User ID: 7");
    expect(rawAccountLabel(null, 7)).toBe("7");
    expect(rawAccountLabel(undefined)).toBe("");
  });
});

describe("accountLabel", () => {
  const shown = { hideUsernames: false, hiddenNameLetters: 2 };
  const hidden = { hideUsernames: true, hiddenNameLetters: 2 };

  it("com nomes à mostra é alias || username", () => {
    expect(accountLabel(ALIASED, shown)).toBe("Main");
    expect(accountLabel(ANN, shown)).toBe("annabelle");
  });

  it("com nomes ocultos mascara o que seria mostrado", () => {
    expect(accountLabel(ANN, hidden)).toBe("an********");
    expect(accountLabel(ALIASED, hidden)).toBe("Ma********");
  });

  it("mascara também o fallback: o User ID identifica a conta tanto quanto o nome", () => {
    expect(accountLabel(undefined, hidden, "User ID: 123456")).toBe("Us********");
    expect(accountLabel(undefined, { hideUsernames: true, hiddenNameLetters: 0 }, 42)).toBe("************");
  });
});

describe("hideAccountAvatar", () => {
  it("esconde o avatar só com nomes ocultos e sem a opção de manter avatares", () => {
    expect(hideAccountAvatar({ hideUsernames: true, showAvatarsWhenHidden: false })).toBe(true);
    expect(hideAccountAvatar({ hideUsernames: true, showAvatarsWhenHidden: true })).toBe(false);
    expect(hideAccountAvatar({ hideUsernames: false, showAvatarsWhenHidden: false })).toBe(false);
  });
});

describe("accountInitial", () => {
  it("com nomes à mostra é a inicial do username, como sempre foi", () => {
    expect(accountInitial(ALIASED, { hideUsernames: false, hiddenNameLetters: 0 })).toBe("A");
    expect(accountInitial(undefined, { hideUsernames: false, hiddenNameLetters: 0 })).toBe("?");
  });

  it("com nomes ocultos não revela a inicial que a prévia não mostraria", () => {
    expect(accountInitial(ANN, { hideUsernames: true, hiddenNameLetters: 0 })).toBe("*");
    expect(accountInitial(ANN, { hideUsernames: true, hiddenNameLetters: 1 })).toBe("A");
  });
});
