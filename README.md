# Karrelage – lecteur passif des paquets de Dofus 3

Karrelage lit les messages que le client Dofus 3 échange avec les serveurs d'Ankama et en tire quelque chose d'utile : prix et lots de
l'hôtel des ventes, bonnes affaires, rentabilité du craft, du brisage et de la transcendance, comparaison de stuffs Dofusbook avec ce
qui est en vente, aide-mémoire des monstres en combat…

Il est **strictement passif** : rien n'est injecté, intercepté ni automatisé dans le jeu. Le dossier du jeu n'est jamais modifié et
Karrelage se contente d'*écouter* le trafic réseau du processus Dofus (via Npcap). Les données restent sur votre ordinateur : les seules
connexions sortantes servent aux mises à jour (GitHub) et à Dofusbook (import de stuffs, icônes).

![Onglet HDV : liste des objets scannés, graphique prix / densité, lots et filtres d'effets](docs/hdv.png)

---

# Pour les utilisateurs

## À installer
1. **Karrelage** – téléchargez `Karrelage_x.y.z_x64-setup.exe` depuis les [releases GitHub](https://github.com/JustNao/karrelage-unity/releases)
   et lancez-le.
2. **[Npcap](https://npcap.com)** – le pilote de capture réseau qu'utilise Karrelage pour écouter le trafic du jeu. Installez-le avec les
   options par défaut. Karrelage démarre sans lui, mais la capture reste indisponible (un bandeau vous le signale).
   La licence de Npcap interdit de le redistribuer : c'est pour cela que Karrelage ne l'inclut pas.
3. **Dofus 3** – rien à configurer ; Karrelage trouve le jeu tout seul.

Windows uniquement (10/11, 64 bits).

## Utilisation
1. Lancez Karrelage et activez l'**interrupteur de capture** dans la barre du haut. Il affiche *En attente de Dofus.exe…* tant que le jeu
   ne tourne pas, puis *Connecté à Dofus*. L'état de l'interrupteur est retenu d'une session à l'autre : vous pouvez laisser Karrelage ouvert en permanence.
2. En jeu, ouvrez des objets à l'**hôtel des ventes** : chaque liste ouverte apparaît dans l'onglet **HDV**. Connectez-vous avec votre
   personnage pendant que la capture tourne pour que Karrelage lise votre inventaire et vos caractéristiques.

| Onglet | Rôle |
| --- | --- |
| **HDV** | Objets scannés regroupés par type (repliables, avec recherche). Par objet : coût de craft comparé à la vente, graphique prix / densité, tableau des lots (prix, dommages / résistances / caractéristiques parfaits, effets au-dessus du max et EXO), *Bonnes affaires* (lots bien en dessous de la tendance de prix, point vert), filtres d'effets à droite (valeurs min, jet parfait, par effet ou par section) et *Épingler* pour comparer n'importe quel lot à un lot épinglé. Familiers et montures : suivis par niveau (prix comparé à la courbe d'expérience) et *statut légendaire*. |
| **HDV › Transcendance** | Les lots qui portent le verrou de transcendance et un effet en plus donnent la valeur d'une *rune de transcendance*. Liste les lots non verrouillés pour lesquels acheter le lot + la rune puis revendre est rentable (investissement, bénéfice, focus par rune) ; *Enregistrer* garde des candidats pour plus tard. |
| **Crafting** | *Runes* : coût de fabrication des runes (ingrédients achetés, ou fabriqués quand c'est moins cher) comparé à leur prix de vente. *Familiers* : coût en nourriture pour monter un familier d'un niveau à un autre. *Montures* : rentabilité de l'extraction des dragodindes, muldos et volkornes (ressources obtenues selon la génération, montures séniles signalées). |
| **Brisage** | Rentabilité du brisage des équipements en runes de forgemagie : coefficient à partir duquel briser chaque objet est rentable (craft ou lots scannés), focus, filtres par type / niveau / objets absents de l'HDV. Les coefficients sont lus lors de vos brisages en jeu (ou saisis à la main) et chaque brisage est historisé. |
| **Stuff** | Importe un stuff Dofusbook (lien, identifiant ou lien court) et calcule ses caractéristiques. Prix du stuff (craft, premier jet, lots choisis), recherche de la combinaison de lots la moins chère qui atteint vos objectifs (PA, PM, PdV…, forgemagie comprise), objets déjà possédés (votre exemplaire ou un jet parfait), liste flottante des objets à scanner. *Exporter mon équipement vers Dofusbook* donne un lien Dofusbook de ce que porte votre personnage (objets, exos, points de caractéristiques et parchemins ; niveau 200). |
| **Prix enregistrés** | Prix unitaire de chaque objet ouvert à l'HDV, par catégorie › type › objet, avec un prix manuel possible. Priorité : votre prix manuel, puis le prix lu à l'HDV, puis la moyenne sur 7 jours envoyée par le jeu. |
| **Combat** | Les monstres du combat en cours avec les notes de stratégie (⚠ danger, 🛡 invulnérabilité, 💡 astuces), leurs particularités (pousse, attire, retraits PA / PM, invocations…) et les interactions dangereuses entre eux ; alerte de poison PA / PM sur votre personnage. Peut s'ouvrir tout seul au début d'un combat et s'afficher dans une petite fenêtre flottante par-dessus le jeu. |
| **Automation** | Règles qui surveillent les scans de l'HDV quel que soit l'onglet ouvert, avec un son et une notification dans la barre du haut (cliquer dessus ouvre le lot). Aujourd'hui : lots portant un exo de dommages qu'une rune de transcendance ajoute, moins chers que la rune et rentables à briser. |
| **Paramètres** | Réglages par onglet (seuil des bonnes affaires de l'HDV, options du combat…). |
| **Paquets** *(mode développeur)* | Inspecteur de paquets : les derniers paquets de chaque type, décodés à l'ouverture de l'onglet. |
| **Proto** *(mode développeur)* | Vérifie que le pack de données correspond au jeu installé et relance la génération des noms (voir plus bas). |

Le mode développeur s'active avec la case *Développeur* en haut à droite.

## Mises à jour
- **Application** : au démarrage, Karrelage vérifie la dernière release GitHub et propose de l'installer depuis la barre du haut
  (téléchargement vérifié, puis l'application redémarre).
- **Pack de données** (noms des messages, objets, effets…) : après une mise à jour de Dofus, le mainteneur publie un nouveau pack ;
  la barre du haut propose de le télécharger, puis de redémarrer.

## Bon à savoir
- Le prix d'une rune n'est à jour que depuis la dernière fois que vous l'avez ouverte à l'hôtel des ventes ; Karrelage prévient quand seule la
  moyenne hebdomadaire est connue.
- Les données sont stockées dans `%APPDATA%\fr.dofus.karrelage\` (objets scannés, prix, stuffs, inventaire, brisages, packs téléchargés).
  Supprimez le dossier pour tout remettre à zéro.
- Le pack de données contient des textes extraits des fichiers d'Ankama : ne le republiez pas publiquement.

---

# Pour les développeurs

Deux parties : l'**application Tauri** (ce que lancent les utilisateurs) et un **pipeline Python** (réservé à l'auteur) qui transforme les fichiers
du jeu en un petit *pack de données* (`datapack/`) chargé par l'application. Les conventions détaillées, les notes sur le protocole et le
fonctionnement de chaque onglet sont dans [CLAUDE.md](CLAUDE.md).

## Organisation du dépôt
```
crates/core      wire.rs (varint, trames, enveloppe), stream.rs (réordonnancement TCP), pcap.rs (wpcap.dll chargée à l'exécution),
                 capture.rs (sockets de Dofus.exe + threads d'écoute, filtre BPF limité aux connexions du jeu)
crates/proto     décodage protobuf dynamique depuis datapack/protocol.desc, noms en clair des messages / champs
crates/data      chargement du pack : objets, effets, types d'objets, panoplies, recettes, monstres, textes, bindings
crates/modules   trait Module + Hub, kit.rs (rôles des bindings, stockage persistant, arguments, outils de test) ;
                 un fichier par fonctionnalité : packets, names (Proto), auction (HDV), prices, brisage, stuff, inventory, combat, update
app/             coque Tauri (src-tauri) + interface Svelte 5 / Vite : src/modules/<id>/ = onglets enregistrés automatiquement,
                 src/lib = composants et magasins partagés ; fenêtres flottantes : scanlist.html, combat.html
pipeline/        fichiers du jeu -> pack de données (python)
fixtures/        paquet HDV synthétique + sortie attendue (test de référence)
dist/            datapack.zip + pack.json publiés (versionnés : c'est ce que téléchargent les applications)
datapack/        généré (ignoré par git)      work/   généré, sauf work/overrides.json (noms confirmés à la main – à garder !)
tools/           outils tiers téléchargés (ignorés par git)
```

## Prérequis
| Pour | Il faut |
| --- | --- |
| L'application | Rust (stable, toolchain MSVC), Node 20+, [prérequis de Tauri 2](https://tauri.app/start/prerequisites/) (WebView2, outils de compilation MSVC) |
| La capture | [Npcap](https://npcap.com) |
| Le pipeline | Python 3.12+, les outils ci-dessous |

### Outils (dans `tools/`, non versionnés)
- **Il2CppInspectorRedux** (`tools/inspector`) – extrait `GameAssembly.dll` + `global-metadata.dat` (métadonnées v39) en DummyDlls. Nécessite **.NET 10**,
  installé localement dans `tools/dotnet10` (`tools/dotnet-install.ps1`) ; définissez `DOTNET_ROOT`.
- **protodec** (`tools/protodec`) – DummyDll → `.proto` obfusqué (un seul fichier : imports circulaires).
- **Bibliothèques Python** dans `tools/pylib` (installées avec `pip install --target tools/pylib grpcio-tools protobuf UnityPy` ; à placer en tête de `sys.path` ;
  la version du runtime protobuf doit correspondre au code généré).
- Protos de la bêta avec les noms en clair : [LuaxY/dofus-unity-protocol-builder](https://github.com/LuaxY/dofus-unity-protocol-builder), cloné dans `work/beta`.

## Après une mise à jour de Dofus
Le plus simple : en mode développeur, l'onglet **Proto** compare le pack au jeu installé et propose *Tout régénérer depuis le jeu*
(`python pipeline/update_names.py --from-game` : copie des fichiers, extraction, protos, données du jeu, noms, pack), *Calculer les noms*
(noms seulement) et *Repack data* (`--repack` : export + paquetage seulement). Redémarrez ensuite. `update_names.py` termine en committant
`dist/` (*Data pack for Dofus x.y.z.w*) sans le pousser.

À la main, depuis la racine du dépôt dans Git Bash (Windows : `export PYTHONUTF8=1`) :

```bash
# copie (sans jamais les modifier) GameAssembly.dll + global-metadata.dat dans work/dump_input/, extraction avec Il2CppInspectorRedux,
# puis DummyDlls -> protos avec protodec : voir from_game() dans update_names.py pour les commandes exactes
python pipeline/build_protos.py             # compile les protos, écrit work/pb/protocol.desc
python pipeline/extract_gamedata.py --refresh   # copie les données et traductions du jeu dans work/gamedata, les lit avec UnityPy
python pipeline/script_names.py             # noms en clair laissés par la table des scripts Unity -> work/script_names.json
python pipeline/match_protos.py             # correspondance structurelle avec la bêta, ancrée sur ces noms -> work/mapping.json
python pipeline/import_reference_names.py <liste.txt>   # facultatif : liste communautaire obfusqué -> clair
python pipeline/export_pack.py              # -> datapack/
python pipeline/package_pack.py             # -> dist/datapack.zip + dist/pack.json
python pipeline/make_fixtures.py            # seulement si le paquet de démo / le formateur a changé
```

Les noms obfusqués (messages `jzs`, champs `fycg`…) et les numéros de champs changent à chaque version. Priorité des noms :
correspondance automatique avec la bêta < `pipeline/reference/message_names.json` < table des scripts < ancres (`match_protos.py` `ANCHORS` :
messages retrouvés à chaque version par leur voisin et la longueur de leur nom) < `work/overrides.json` (indexé par les noms obfusqués, donc
à revoir après chaque mise à jour). `pipeline/bindings.json` associe des *rôles* (champs des listes HDV, des effets, de l'inventaire…) aux noms
de champs obfusqués : il faut en général le mettre à jour, et l'onglet **Proto** signale les rôles qui ne se résolvent plus. Le code de
l'application ne contient jamais de nom obfusqué en dur.

Alternative en ligne de commande pour enregistrer des paquets : `python pipeline/sniff.py` (ajoutez `--raw` pour l'hexadécimal du contenu) écrit du JSONL.

## Lancer, tester, construire l'application
```bash
cargo test                      # tests unitaires + test de référence (sortie Rust == sortie du formateur python ; ignorés sans datapack/)
cd app && npm install
npm run tauri dev               # application de développement (cochez « Développeur » pour les onglets Paquets et Proto)
npm run check                   # svelte-check
npm run tauri build             # installeur NSIS dans target/release/bundle/nsis/, inclut datapack/ comme ressource
```
L'installeur nécessite d'avoir généré `datapack/` auparavant (ou de l'avoir décompressé depuis `dist/datapack.zip`).

## Publier
- **Un pack de données** : poussez sur `main` le commit de `dist/` ; les applications lisent `dist/pack.json` au démarrage et proposent le nouveau pack.
- **Une version de l'application** : augmentez la version dans `Cargo.toml` (workspace), `app/src-tauri/tauri.conf.json` et `app/package.json`, puis
  `git tag vX.Y.Z && git push origin vX.Y.Z`. `.github/workflows/release.yml` construit l'installeur (en reconstruisant `datapack/` depuis `dist/`)
  et le joint à une release GitHub, que les applications proposent au démarrage. `.github/workflows/ci.yml` lance `cargo fmt --check`, `cargo clippy`,
  `cargo test` et `svelte-check` à chaque push / PR.

## Ajouter une fonctionnalité
1. Rust : nouveau fichier dans `crates/modules/src/` qui implémente `Module` (`interest` = noms en clair des messages, `on_message`, `call`, et au besoin
   `format` / `init` / `wants` / `needs_decoded`), enregistré dans `all()` de `lib.rs`. Utilisez le kit : `Role` pour les bindings, `Store` pour l'état
   persistant, `arg_*` / `unknown` dans `call`, `testkit` dans les tests.
2. Interface : nouveau dossier `app/src/modules/<id>/` avec `index.ts` (`{id, title, order, devOnly?, component}`) et `View.svelte` ; il devient un onglet
   automatiquement. On parle au backend avec `call(module, method, args)` et `listenAll({"<module>:<event>": fn})`. Les briques partagées (stockage
   local, listes synchronisées, colonnes redimensionnables, fenêtres flottantes, composants) sont décrites dans [CLAUDE.md](CLAUDE.md).
3. Nouveaux messages : confirmez leurs noms dans `work/overrides.json`, ajoutez les rôles de leurs champs dans `pipeline/bindings.json`, relancez `export_pack.py`.

## Principes
- **Passif uniquement.** Ne jamais modifier le dossier ni le processus du jeu ; uniquement des copies ; capture = écoute via Npcap.
- **Léger par défaut** : rien de lourd ni d'écriture disque sur le thread de capture, aucun décodage qu'aucun module ne lit.
- Les noms obfusqués sont propres à chaque version : utilisez les noms en clair ou les rôles de `bindings.json`.
- Un commit par fonctionnalité qui marche.
