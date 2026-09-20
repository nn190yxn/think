import { useCallback, useEffect, useMemo, useState } from "react";
import { useCommand } from "./app/ipc";
import { usePreferences } from "./app/preferences";
import { useRealmRoute } from "./app/routing";
import { useTheme } from "./app/theme";
import { ambientGlow, computeFurnaceTemp } from "./domain/furnace";
import { realmOf } from "./domain/realms";
import { FurnaceTemp } from "./components/FurnaceTemp";
import { EmberLayer } from "./components/EmberLayer";
import { Spine, defaultSpineEntries } from "./components/Spine";
import { ThemeToggle } from "./components/ThemeToggle";
import { CommandPalette } from "./components/CommandPalette";
import { ObserveRealm } from "./realms/ObserveRealm";
import { CouncilRealm } from "./realms/CouncilRealm";
import { RefineRealm } from "./realms/RefineRealm";
import { VaultRealm } from "./realms/VaultRealm";
import { SelfRealm } from "./realms/SelfRealm";

export function App() {
  const [theme, setTheme] = useTheme();
  const [preferences, updatePreferences] = usePreferences();
  const [realm, navigate] = useRealmRoute();
  const [seed, setSeed] = useState<string | null>(null);
  const [councilSession, setCouncilSession] = useState<string | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  // 「我」分成长与设置两处：顶部「设置」按钮直进设置，走脊柱或命令面板则默认看成长。
  const [selfView, setSelfView] = useState<"growth" | "system">("growth");
  const snapshot = useCommand("furnace_snapshot", {});
  const networking = useCommand("networking_get", {});
  const offline = networking.data !== true;

  // Ctrl/Cmd + K 是重度用户的主通道，结构化指令与自然语言共用同一个输入。
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((current) => !current);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // 走到「我」默认看成长，只有顶部「设置」按钮会指定看设置。
  const goRealm = useCallback(
    (next: Parameters<typeof navigate>[0]) => {
      if (next === "self") {
        setSelfView("growth");
      }
      navigate(next);
    },
    [navigate],
  );

  // 从星图把节点内容变成会诊议题，是全应用的主链路之一。
  const seedCouncil = useCallback(
    (question: string) => {
      setSeed(question);
      setCouncilSession(null);
      navigate("council");
    },
    [navigate],
  );

  // 余烬把洞察转为会诊：直接打开已建好的会话。
  const openCouncilSession = useCallback(
    (sessionId: string, question: string) => {
      setSeed(question);
      setCouncilSession(sessionId);
      navigate("council");
    },
    [navigate],
  );

  const temp = useMemo(() => {
    const data = snapshot.data;
    if (!data) {
      return 0;
    }
    return computeFurnaceTemp({
      activeNodes: data.activeNodes,
      totalNodes: data.totalNodes,
      recentCaptures: data.recentCaptures,
      recentCouncils: data.recentCouncils,
    });
  }, [snapshot.data]);

  // 炉温直接驱动环境光强度，让冷热一眼可见。
  useEffect(() => {
    document.documentElement.style.setProperty("--furnace-glow", String(ambientGlow(temp)));
    document.body.dataset["tempBand"] = String(temp);
  }, [temp]);

  const entries = useMemo(() => defaultSpineEntries(), []);

  return (
    <div className="shell">
      <Spine entries={entries} current={realm} onSelect={goRealm} />
      <main className="stage">
        <div className="stage__bar">
          <span className="stage__crumb">
            {realmOf(realm).sigil} · {realmOf(realm).title}
          </span>
          <button
            className="stage__palette"
            type="button"
            aria-label="打开设置"
            onClick={() => {
              setSelfView("system");
              navigate("self");
            }}
          >
            设置
          </button>
          <button
            className="stage__palette"
            type="button"
            onClick={() => setPaletteOpen(true)}
          >
            命令面板 <span className="mono">Ctrl K</span>
          </button>
          <ThemeToggle theme={theme} onChange={setTheme} />
        </div>
        <div className="stage__body" key={realm}>
          {realm === "observe" ? <ObserveRealm temp={temp} onSeed={seedCouncil} /> : null}
          {realm === "council" ? (
            <CouncilRealm seed={seed ?? undefined} sessionId={councilSession ?? undefined} />
          ) : null}
          {realm === "refine" ? <RefineRealm /> : null}
          {realm === "vault" ? <VaultRealm /> : null}
          {realm === "self" ? (
            <SelfRealm
              theme={theme}
              onThemeChange={setTheme}
              preferences={preferences}
              onPreferencesChange={updatePreferences}
              view={selfView}
              onViewChange={setSelfView}
              onGoVault={() => goRealm("vault")}
            />
          ) : null}
        </div>
      </main>
      <FurnaceTemp temp={temp} offline={offline} />
      <EmberLayer
        onOpenCouncil={(session) => openCouncilSession(session.id, session.question)}
      />
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onNavigate={goRealm}
        onSeedCouncil={seedCouncil}
        theme={theme}
        onThemeChange={setTheme}
        preferences={preferences}
        onPreferencesChange={updatePreferences}
      />
    </div>
  );
}
