# Stockcorner JC-4s Tuner — Serieel Interface via USB

## Overzicht

De Stockcorner JC-Control kastje (bij de JC-4s antenne tuner) is aangepast om extern aangestuurd te worden via een USB-naar-serieel adapter. Er worden **geen seriële data** verstuurd — alleen de **modem control lines** (RTS/CTS) worden gebruikt als digitale signalen.

## Hardware aansluitingen

### Twee signalen

| Signaal | Richting | Functie |
|---------|----------|---------|
| **RTS** (Request To Send) | PC → Tuner | Simuleert het indrukken van de Start/Tune knop |
| **CTS** (Clear To Send) | Tuner → PC | Leest de rode Tune LED status uit |

### JC-Control kast aanpassing

1. **Start knop (RTS)**: De Start-knop op de JC-Control kan via een simpel schakelcircuit kortgesloten worden. Het RTS signaal van de USB-serieel adapter schakelt dit circuit, waardoor de tuner begint met tunen — alsof je de knop indrukt.

2. **Tune LED (CTS)**: De rode Tune LED op de JC-Control wordt uitgelezen. Dit signaal gaat naar de CTS lijn van de USB-serieel adapter. Zolang de tuner bezig is met tunen brandt de LED (CTS = HIGH), wanneer het tunen klaar is gaat de LED uit (CTS = LOW).

### USB-serieel adapter

Een standaard USB-naar-serieel (TTL) printje verbindt deze twee signalen met de server PC. Alleen de RTS en CTS lijnen worden gebruikt, plus GND. De TX/RX data lijnen worden niet aangesloten.

```
USB-Serieel adapter          JC-Control kast
─────────────────          ─────────────────
RTS ──────────────────────→ Start knop (schakelcircuit)
CTS ←────────────────────── Tune LED (uitgelezen)
GND ──────────────────────→ GND
```

## Software protocol

De ThetisLink Server (`tuner.rs`) opent de COM-poort op 9600 baud en gebruikt uitsluitend de modem control lines:

### Initialisatie

1. DTR HIGH zetten (voeding/ready signaal)
2. RTS HIGH voor 200ms, dan RTS LOW — wake-up puls voor de JC-4s

### Tune sequentie

```
Stap  Actie                          Signaal
────  ─────                          ───────
1     RTS HIGH                       Tuner voorbereiden
2     Wacht 150ms
3     ZZTU1 naar Thetis (CAT)        Tune carrier AAN (CW draaggolf)
4     Wacht 500ms (carrier opstart)
5     RTS LOW                        Start het tunen
6     Wacht op CTS = TRUE            Tuner is begonnen
7     Wacht op CTS = FALSE           Tunen klaar (LED uit)
8     ZZTU0 naar Thetis (CAT)        Tune carrier UIT
```

### Timeout en abort

- Als CTS niet binnen 3 seconden TRUE wordt na RTS LOW → timeout
- Als het tunen langer duurt dan 30 seconden → timeout
- Bij abort: ZZTU0 sturen en RTS LOW zetten

### Safe tune (PA bescherming)

Als er een eindversterker (SPE Expert of RF2K-S) is aangesloten, wordt deze automatisch in Standby gezet voordat het tunen begint, en na afloop weer in Operate.

## Server configuratie

In het ThetisLink Server configuratiebestand:

```
tuner_port=COM5
tuner_enabled=true
```

Of via command line:

```
ThetisLink-Server.exe --tuner-port COM5
```

## Status in de UI

De tuner status wordt weergegeven in zowel de server UI als de remote clients:

| State | Betekenis | Kleur |
|-------|-----------|-------|
| 0 - Idle | Klaar voor tunen | Grijs |
| 1 - Tuning | Bezig met tunen | Blauw |
| 2 - Done OK | Succesvol getuned | Groen |
| 3 - Timeout | Tunen duurde te lang / CTS niet gedetecteerd | Amber |
| 4 - Aborted | Tunen afgebroken | Amber |
| 5 - Done (assumed) | Tuner waarschijnlijk al getuned (CTS-puls compensatie) | Olijfgroen |

De "Done OK" en "Done (assumed)" status worden "stale" (grijs) als de VFO meer dan 25 kHz verschuift ten opzichte van de getuunde frequentie.

## Bekende hardware-beperking: CTS-puls bij snel tunen

### Het probleem

De JC-Control box gebruikt een zelfgebouwd schakelcircuit om de Tune LED status door te geven aan de CTS lijn van de USB-serieel adapter. Dit circuit werkt betrouwbaar wanneer de JC-4s daadwerkelijk moet tunen (LED brandt 1-9 seconden). Echter, wanneer de tuner al getuned is voor de huidige frequentie, geeft de JC-4s een zeer korte LED-puls (of helemaal geen puls). Het schakelcircuit kan deze korte puls niet doorgeven aan de FTDI adapter, waardoor CTS nooit TRUE wordt.

Dit resulteert in een false timeout: de tuner is klaar, maar ThetisLink denkt dat het tunen niet is gestart.

### De oplossing: CTS-puls compensatie

In de server configuratie bij de JC-4s Tuner is een checkbox **"CTS-puls compensatie"** beschikbaar:

- **Uit (standaard):** Origineel gedrag. CTS niet TRUE na 3 seconden = TIMEOUT (grijs). Gebruik deze instelling als je hardware-implementatie een betrouwbaar CTS signaal levert bij elke tune-actie.

- **Aan:** Compensatie actief. Als CTS niet TRUE wordt binnen 500ms, neemt ThetisLink aan dat de tuner al getuned is (state "Done ~", olijfgroen). De tune carrier wordt netjes uitgeschakeld. Gebruik deze instelling als je regelmatig false timeouts ziet bij het opnieuw tunen op dezelfde frequentie.

### Hoe herken je het probleem?

Als je na het klikken op "Tune" regelmatig een grijze knop (timeout) ziet terwijl de tuner LED al uit is en de SWR goed is, dan heb je waarschijnlijk dit probleem. Schakel "CTS-puls compensatie" in.

### Technische details

Met compensatie aan:
- Eerste 200ms na tune-trigger: agressieve CTS polling elke 5ms (vangt marginale pulsen)
- Na 500ms zonder CTS TRUE: aannemen al getuned (DONE_ASSUMED)
- ZZTU0 (tune carrier uit) wordt altijd gestuurd
- Het verschil is zichtbaar als olijfgroen "Tune ~" in plaats van groen "Tune OK"
