# Rationale: redirección genérica de control

Mounts debe conocer `MountedOn`, pero Movement es dueño de `Intents` y de su
pipeline. Por eso Mounts no escribe los intents del horse ni neutraliza los
del jinete directamente.

Mounts emite `ActorLinkRequestMessage::Attach` al montar y
`ActorLinkRequestMessage::Detach` al desmontar. Movement instala o retira
atómicamente `KinematicAttachment`, `ControlRedirect`, collider y gate, y
responde con `ActorLinkResultMessage`; Mounts solo confirma su relación desde
un ack aceptado, y los requests se aplican el mismo tick en que llegan.
Después de `MovementSet::ReadIntents`,
`MovementSet::ControlRedirect` copia planar, sprint y salto normalizados al
actor controlado, descarta acciones incompatibles y deja los intents del
controller en default. Detach neutraliza también al controlled actor. Si la
pose de emergencia no está validada, `PendingSafeRecovery` conserva colisión y
locomoción deshabilitadas hasta que Movement encuentre una pose sin overlap.

El contrato no contiene tipos de Mounts, por lo que también sirve para
vehículos y compañeros. El orden `ApplyExternal -> ReadIntents ->
ControlRedirect` garantiza transferencia en el mismo tick físico sin que
`brain::read_intents` conozca relaciones externas.
