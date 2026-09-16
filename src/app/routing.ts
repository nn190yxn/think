import { useCallback, useEffect, useState } from "react";
import { DEFAULT_REALM, isRealmKey, type RealmKey } from "../domain/realms";

const HASH_PREFIX = "#/";

function readHash(): RealmKey {
  if (typeof window === "undefined") {
    return DEFAULT_REALM;
  }
  const raw = window.location.hash.replace(HASH_PREFIX, "");
  return isRealmKey(raw) ? raw : DEFAULT_REALM;
}

/**
 * 境界切换靠 hash 表达，这样视点可被记住，也不需要引入路由库。
 */
export function useRealmRoute(): readonly [RealmKey, (next: RealmKey) => void] {
  const [realm, setRealm] = useState<RealmKey>(readHash);

  useEffect(() => {
    const onHashChange = () => setRealm(readHash());
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  const navigate = useCallback((next: RealmKey) => {
    const target = `${HASH_PREFIX}${next}`;
    if (window.location.hash !== target) {
      window.location.hash = target;
    }
    setRealm(next);
  }, []);

  return [realm, navigate] as const;
}
