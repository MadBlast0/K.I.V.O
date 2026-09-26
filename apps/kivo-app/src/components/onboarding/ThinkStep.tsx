/**
 * Setup's Think page (UX-34, owner 2026-09-26): every kind of brain can be connected here, and
 * connecting one never hides the others, so the user can add several and choose the one KIVO
 * uses (with its model and reasoning level) at the top.
 *
 * - Sign in: OpenRouter in the browser (free models), and the AI apps on this PC (Claude Code,
 *   Gemini CLI, Codex…): Connect when signed in, Sign in when not, Install when missing.
 * - On this PC: local servers found running (Ollama, LM Studio…), or one by its address.
 * - API key: each service, its key typed here, tested before it's saved (write-only, SEC-19).
 *
 * Nothing is connected unasked (DISC-03); the one setup suggests for this PC is marked (UX-36).
 */
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ActiveBrain } from "../brains/ActiveBrain";
import { InstallSheet } from "../agents/InstallSheet";
import { brainColor, monogram, type BrainsList, type Catalog, type DiscoverySection } from "../../ipc/brains";
import { Method, type SetupAdvice } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Button, Monogram, Pill, Segmented, Spinner, TextField, useToast } from "../ui";
import { Reasons } from "./steps";

type Way = "signIn" | "local" | "key";
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function ThinkStep({ advice }: { advice: SetupAdvice | null }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [list, setList] = useState<BrainsList | null>(null);
  const [found, setFound] = useState<DiscoverySection["items"] | null>(null);
  const [way, setWay] = useState<Way>("signIn");
  const [installing, setInstalling] = useState<string | null>(null);
  const [keyFor, setKeyFor] = useState<string | null>(null);
  const [key, setKey] = useState("");
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState<string | null>(null);

  const load = useCallback(() => {
    void request<BrainsList>(Method.brainsList)
      .then(setList)
      .catch(() => {});
  }, [request]);
  const look = useCallback(() => {
    void Promise.all(
      ["cli", "local"].map((section) =>
        request<DiscoverySection>(Method.brainsRefresh, { section })
          .then((s) => s.items)
          .catch(() => []),
      ),
    ).then((lists) => setFound(lists.flat()));
  }, [request]);
  useEffect(() => {
    if (!connected) return;
    load();
    look();
    void request<Catalog>(Method.brainsCatalog)
      .then(setCatalog)
      .catch(() => {});
  }, [connected, load, look, request]);

  const run = (id: string, work: Promise<unknown>) => {
    setBusy(id);
    work
      .then(load)
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setBusy(null));
  };
  const isOn = (id: string) => list?.connected.some((b) => b.id === id) ?? false;
  const suggested = (id: string) => advice?.brain === id && !isOn(id);
  const done = (id: string) => (
    <>
      <Pill tone="success">{t("onboarding.brain.connected")}</Pill>
      <Button size="sm" variant="plain" onClick={() => run(id, request(Method.brainsDisconnect, { id }))}>
        {t("onboarding.brain.disconnect")}
      </Button>
    </>
  );
  const row = (id: string, name: string, detail: string, end: ReactNode, free?: string | null) => (
    <li key={id} className="k-think__row">
      <Monogram text={monogram(name)} color={brainColor(id)} />
      <div className="k-think__text">
        <b>
          {name}
          {suggested(id) && <span className="k-chip k-chip--accent">{t("onboarding.brain.suggested")}</span>}
          {free && <span className="k-chip k-chip--good">{t("onboarding.brain.free")}</span>}
        </b>
        <span>{detail}</span>
      </div>
      <div className="k-think__end">{busy === id ? <Spinner label={t("onboarding.brain.working")} /> : end}</div>
    </li>
  );

  const entries = catalog?.brains ?? [];
  const cli = entries.filter((e) => e.kind === "cli");
  const oauth = entries.filter((e) => e.signIn === "oAuth");
  const keyed = entries.filter((e) => e.signIn === "apiKey");
  const locals = (found ?? []).filter((i) => i.data.url);
  const agentFound = (id: string) => found?.find((i) => i.id === id);

  return (
    <div className="k-think">
      {list && <ActiveBrain list={list} onChange={setList} />}

      <Segmented<Way>
        label={t("onboarding.brain.ways")}
        value={way}
        onChange={setWay}
        options={[
          { value: "signIn", label: t("onboarding.brain.way.signIn") },
          { value: "local", label: t("onboarding.brain.way.local") },
          { value: "key", label: t("onboarding.brain.way.key") },
        ]}
      />
      <p className="k-think__hint">{t(`onboarding.brain.wayHint.${way}`)}</p>

      {!catalog ? (
        <Spinner label={t("onboarding.brain.searching")} />
      ) : (
        <ul className="k-think__list">
          {way === "signIn" && (
            <>
              {oauth.map((e) =>
                row(
                  e.id,
                  e.name,
                  t("onboarding.brain.openRouterHint"),
                  isOn(e.id) ? (
                    done(e.id)
                  ) : (
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => run(e.id, request(Method.brainsSignIn, { id: e.id }))}
                    >
                      {t("onboarding.brain.signIn")}
                    </Button>
                  ),
                  e.free,
                ),
              )}
              {cli.map((e) => {
                const here = agentFound(e.id);
                // Still looking on this PC: say so, and wait to offer anything.
                if (found === null) {
                  return row(
                    e.id,
                    e.name,
                    t("onboarding.brain.checking"),
                    <Spinner label={t("onboarding.brain.checking")} />,
                    e.free,
                  );
                }
                const detail = !here
                  ? t("onboarding.brain.notInstalled")
                  : here.data.signedIn === false
                    ? t("onboarding.brain.needsSignIn")
                    : t("onboarding.brain.onThisPc", { version: here.data.version ?? "" });
                const end = isOn(e.id) ? (
                  done(e.id)
                ) : !here || here.data.needsAdapter ? (
                  <Button size="sm" onClick={() => setInstalling(e.id)}>
                    {t("onboarding.brain.install")}
                  </Button>
                ) : here.data.signedIn === false ? (
                  <Button size="sm" onClick={() => run(e.id, request(Method.brainsSignIn, { id: e.id }))}>
                    {t("onboarding.brain.signIn")}
                  </Button>
                ) : (
                  <Button
                    size="sm"
                    variant="primary"
                    onClick={() => run(e.id, request(Method.brainsConnect, { id: e.id }))}
                  >
                    {t("onboarding.brain.connect")}
                  </Button>
                );
                return row(e.id, e.name, detail, end, e.free);
              })}
            </>
          )}

          {way === "local" && (
            <>
              {found === null ? (
                <li className="k-think__empty">{t("onboarding.brain.searching")}</li>
              ) : (
                locals.length === 0 && <li className="k-think__empty">{t("onboarding.brain.noLocal")}</li>
              )}
              {locals.map((i) =>
                row(
                  i.id,
                  i.data.name ?? i.id,
                  t("onboarding.brain.runningHere", { count: i.data.models?.length ?? 0 }),
                  isOn(i.id) ? (
                    done(i.id)
                  ) : (
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => run(i.id, request(Method.brainsConnect, { id: i.id, baseUrl: i.data.url ?? "" }))}
                    >
                      {t("onboarding.brain.connect")}
                    </Button>
                  ),
                  t("onboarding.brain.free"),
                ),
              )}
              <li className="k-think__row k-think__row--form">
                <TextField
                  value={url}
                  placeholder="http://127.0.0.1:8000/v1"
                  aria-label={t("onboarding.brain.serverUrl")}
                  onChange={(e) => setUrl(e.target.value)}
                />
                <Button
                  size="sm"
                  disabled={!url.trim()}
                  onClick={() => {
                    const address = url.trim();
                    run(
                      "custom",
                      request(Method.brainsConnect, {
                        id: `custom-${new URL(address).host.replace(/[^a-z0-9]+/gi, "-").toLowerCase()}`,
                        name: new URL(address).host,
                        baseUrl: address,
                        local: true,
                      }).then(() => setUrl("")),
                    );
                  }}
                >
                  {t("onboarding.brain.addServer")}
                </Button>
              </li>
            </>
          )}

          {way === "key" &&
            keyed.map((e) => (
              <li key={e.id} className="k-think__block">
                <ul className="k-think__inner">
                  {row(
                    e.id,
                    e.name,
                    isOn(e.id) ? t("onboarding.brain.keySaved") : t("onboarding.brain.keyHint"),
                    isOn(e.id) ? (
                      done(e.id)
                    ) : (
                      <Button
                        size="sm"
                        onClick={() => {
                          setKeyFor(keyFor === e.id ? null : e.id);
                          setKey("");
                        }}
                      >
                        {t("onboarding.brain.addKey")}
                      </Button>
                    ),
                    e.free,
                  )}
                </ul>
                {keyFor === e.id && !isOn(e.id) && (
                  <div className="k-think__key">
                    <TextField
                      type="password"
                      autoComplete="off"
                      value={key}
                      placeholder={t("onboarding.brain.keyPlaceholder", { name: e.name })}
                      aria-label={t("onboarding.brain.apiKey", { name: e.name })}
                      onChange={(ev) => setKey(ev.target.value)}
                    />
                    <Button
                      size="sm"
                      variant="primary"
                      disabled={!key.trim()}
                      onClick={() =>
                        run(
                          e.id,
                          request(Method.brainsConnect, { id: e.id })
                            .then(() => request(Method.brainsSetKey, { id: e.id, key: key.trim() }))
                            .then(() => {
                              setKeyFor(null);
                              setKey("");
                            }),
                        )
                      }
                    >
                      {t("onboarding.brain.testAndSave")}
                    </Button>
                  </div>
                )}
              </li>
            ))}
        </ul>
      )}
      <Reasons advice={advice && { ...advice, reasons: advice.reasons.slice(0, 1) }} />
      {installing && (
        <InstallSheet
          id={installing}
          agent
          onClose={() => setInstalling(null)}
          onDone={() => {
            setInstalling(null);
            look();
          }}
        />
      )}
    </div>
  );
}
