import type { ReactNode } from "react";
import { realmOf, type RealmKey } from "../domain/realms";

/**
 * 境界外壳。五个境界共用同一结构：题头、主体、右侧静态栏。
 * 视点变换只换主体内容，题头与炉温保持不动，保持空间连续感。
 */
export function RealmShell({
  realm,
  aside,
  children,
}: {
  readonly realm: RealmKey;
  readonly aside?: ReactNode;
  readonly children: ReactNode;
}) {
  const meta = realmOf(realm);
  return (
    <section className="realm" data-realm={realm}>
      <header className="realm__head">
        <span className="realm__sigil" aria-hidden="true">
          {meta.sigil}
        </span>
        <div>
          <h1 className="realm__title">{meta.title}</h1>
          <p className="realm__subtitle">{meta.subtitle}</p>
        </div>
        {aside ? <div className="realm__aside">{aside}</div> : null}
      </header>
      <div className="realm__body">{children}</div>
    </section>
  );
}
