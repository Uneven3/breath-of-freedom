# Rationale: combos por arma como datos, fases como estado

Cómo se modelan las cadenas de golpes de Combate (`docs/architecture/
combat.md` § El modelo de combos) y por qué esa forma.

## La decisión

Un combo es **datos** (`WeaponProfile.combo: [AttackStep]`), no estados ni
código: cada `AttackStep` describe un golpe (timings de windup/active/
recovery, ventana de encadenado, multiplicador, alcance, arco). El
`CombatState` solo conoce **fases** (`Windup/Active/Recovery`), que se
repiten para cada paso; *cuál* paso está corriendo vive en `ComboLocal.step`
(componente por-actor, patrón `JumpLocal`).

```
espada 1 mano:  [corte_h]──▶[corte_v]──▶[estocada]──▶ reset
                 W·A·R ▲     W·A·R ▲      W·A·R
                       └─ buffer + chain_window ─┘
```

## Por qué el paso NO va en el enum

La alternativa era `CombatState::Attacking { step: u8 }` o variantes
`Attack1/Attack2/Attack3`. Se descartó por tres razones:

1. **El enum garantiza exclusividad de fase, no de contenido** — la misma
   regla de `LocomotionState` (`rationale/per-entity-state-idioms.md`).
   `Windup` del paso 1 y del paso 3 son la misma fase con distinto tuning;
   duplicarla en variantes multiplica los brazos del dispatcher sin agregar
   exclusividad nueva.
2. **Armas con cadenas de distinto largo** no obligan a variantes nuevas:
   la lanza de 5 golpes y el mazo de 2 usan el mismo enum. Agregar un arma
   es agregar *datos* (un preset const), cero código nuevo — abierto/cerrado
   (Constitución §2).
3. El `ProposalBuffer` compartido exige `Copy + Eq` baratos y el arbitraje
   compara estados; un payload dentro del enum convierte "¿es el mismo
   estado?" en una pregunta ambigua (¿`Attacking{1}` == `Attacking{2}`?).

## Por qué el buffer de input (y de dónde viene)

Sin buffer, encadenar exige presionar exactamente dentro de la ventana de
recovery — a 60 Hz eso es frustración, no peso. Con buffer, presionar
durante `Active`/`Recovery` queda registrado y el encadenado dispara al
abrirse la ventana. Es *exactamente* el patrón ya validado del jump buffer
de Movement (`JumpLocal.buffer`): el peso viene de que el **windup no es
cancelable**, no de castigar el timing del botón.

## Por qué el sweep enmascara a `GameLayer::Actor`

Los sensores de Movement enmascaran a `Default` (solo mundo); los hitboxes
de Combate enmascaran a `Actor` (solo cuerpos). Misma infraestructura de
capas, roles invertidos — un golpe no "choca" contra una pared (fase 1; el
clank contra el mundo puede agregarse como feedback después) y un sensor no
"ve" un cuerpo. Ver `docs/architecture/movement.md` § capas.

## Qué valida esto del núcleo compartido

`proposal-arbitration-core.md` apostó a que el `ProposalBuffer` genérico
serviría para Combate. Este diseño lo somete a su primera prueba real:
motores `attack/guard/parry/aim/stagger` proponiendo contra `idle` default,
con `Staggered` entrando por prioridad alta. Si el núcleo necesita cambiar
(p. ej. otra semántica de `Continuation` para fases encadenadas), se cambia
el núcleo con un rationale, no se bifurca.
