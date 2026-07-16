# Ticket: `combat-melee-combo` — IMPLEMENTADO (pendiente checkpoint jugado)

## Sistema(s)

Combat (sobre `combat-scaffolding`), con placeholder de VFX en Visuals y
línea de estado en Debug.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/architecture/combat.md` §§ Modelo de combos / Detección de golpes.
3. `docs/architecture/rationale/combat-combo-chains.md`.
4. `docs/tickets/combat-scaffolding.md`.

## Qué se construyó

- `WeaponProfile::GRAYBOX_SWORD`: 3 pasos (dos cortes rápidos + finisher
  pesado que no encadena), valores de primera pasada — **se afinan en el
  checkpoint jugado**, no antes (Constitución §10/§11). `WeaponClass` quedó
  diferido a `combat-weapon-classes` (ningún sistema lo lee todavía).
- Motor `attack` (`combat/motors/attack.rs`):
  - `propose` es la máquina de fases: arranque desde `Idle`
    (PlayerRequested), hold/advance por timers (`Forced`, pesos
    HOLD < ADVANCE < CHAIN), **encadenado por buffer** dentro de
    `chain_window_secs` (patrón jump-buffer), recovery vencido = silencio →
    `Idle` gana.
  - `tick_phase_clock` (en el dispatcher) avanza el reloj de fase y detecta
    la entrada a cada fase (reset + clear del swing al entrar a `Active`).
  - `sweep_active_swings`: **sistema aparte del dispatcher** — necesita leer
    transforms de *otros* actores, y la query mutable del dispatcher no
    puede aliasar eso. Esfera de radio `reach/2` centrada medio-reach al
    frente, **enmascarada a `GameLayer::Actor`** (inverso exacto del sensing
    de Movement), filtro de arco horizontal (`arc_deg`), dedup por
    `ActiveSwing` (capacidad fija, un golpe por objetivo por swing).
  - `resolve_melee_hits`: daño puro (`final_damage`: sneakstrike ×8 solo
    contra objetivo no-alertado + atacante en Sneak; objetivo sin
    `Awareness` cuenta como alertado), **cue de log como placeholder de
    `health::DamageRequestMessage`** hasta `health-core`, y
    `enemies::DirectThreatMessage` — pegarle al bokobo lo agroa hoy.
- VFX de swing (`visuals.rs`): sector de arco translúcido (~0.16 s) al
  entrar en `Active`, geometría del paso real (`reach`/`arc_deg`).
  **Divergencia del plan documentada**: es lectura read-only de
  `Changed<CombatState>` (patrón del tint del bokobo), no `CueMessage` — el
  cue actual no lleva payload posicional; migrar cuando VFX/SFX reales lo
  pidan.
- HUD: línea `combat: <estado>` junto al estado locomotor.

## Checkpoint de feeling (pendiente — el usuario juega)

Click izquierdo: cadena de 3 golpes con peso (windup incancelable);
spamear el botón debe encadenar sin frustrar; pegarle al bokobo debe
agroarlo (violeta→rojo) aunque no te haya visto; sneakstrike desde atrás en
sigilo debe loguear `(SNEAKSTRIKE)` ×8; el sprint debe cortarse al atacar.

## Definición de terminado

- [x] fmt/clippy/test limpios.
- [x] Invariantes §11: fases hold/advance, encadenado solo en ventana,
      finisher nunca encadena, recovery vencido decae por silencio, no-bleed
      de `ComboLocal` entre actores, dedup de `ActiveSwing`, matemática de
      sneakstrike, arco acepta frente y rechaza flancos.
- [ ] Checkpoint jugado + tuning de la cadena.
