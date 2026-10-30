# OpenSmith - Jeu de guitare macOS natif + Générateur MP3-to-Tab IA

## 1. Vision & Objectif
Application macOS native (Apple Silicon M-series) permettant de glisser n'importe quel fichier `.mp3` / `.wav` (ou d'importer une tablature Guitar Pro `.gp`) pour jouer par-dessus avec une guitare électrique branchée via une interface audio, dont le câble USB Real Tone Cable, avec rendu 3D type "Note Highway", simulation d'ampli temps réel et détection de pitch à ultra-basse latence.

## 2. Contraintes Matérielles & OS
- **Cible :** macOS (Apple Silicon arm64).
- **Audio Input :** CoreAudio en mode exclusif / Low-Latency (Buffer 64 ou 128 samples @ 48kHz, latence < 5ms). Auto-détection du périphérique `Rocksmith USB Guitar Adapter`.
- **Rendu 3D :** 60/120 FPS constants (Metal / WGPU / Three.js WebGPU).

## 3. Architecture Découplée (2 Moteurs)

### Module A : `Engine-RT` (Temps Réel Audio/DSP & UI) - Rust + Tauri v2 (ou C++/Swift)
Le traitement du signal de la guitare ne doit JAMAIS passer par du Python ou du JavaScript.
1. **Audio Graph (Rust `cpal` + `fundsp` / `neural-amp-modeler`) :**
   - Entrée : Flux mono 48kHz de la guitare.
   - DSP : Noise Gate -> Pitch Detector -> Neural Amp Modeler (fichiers `.nam`) + Impulse Response (IR Cabinet `.wav`) -> Mixage avec le backing track -> Sortie Casque/Enceintes.
2. **Détection de Notes Temps Réel :**
   - Algorithme YIN / McLeod Pitch Method (MPM) en Rust pour les notes simples (latence < 10ms).
   - FFT / Chromagramme temps réel pour la validation des accords (polyphonique).
3. **Moteur Visuel "Note Highway" (Frontend WebGPU / Three.js) :**
   - Code couleur des cordes :
     - Mi grave (E) = Rouge | La (A) = Jaune | Ré (D) = Bleu | Sol (G) = Orange | Si (B) = Vert | Mi aigu (e) = Violet.
   - Caméra dynamique qui zoome/se déplace sur les cases actives du manche.
   - Gestion des techniques : Sustain (traînées), Hammer-on / Pull-off (HO/PO), Slides, Bends, Palm Mute, Accords.
   - Mode "Loop Trainer" : ralentissement temporel de l'audio (Time-Stretching sans changer le pitch) et boucle sur une section précise.

### Module B : `Pipeline-Ingest` (MP3 vers chart jouable) - Python CLI (Accélération MPS / CoreML)
Pipeline exécuté à l'import d'un fichier MP3 :
1. **Séparation de pistes (Stem Separation) :** Utilisation de `Demucs` (modèle `htdemucs_6s` qui isole spécifiquement : Voix, Basse, Batterie, **Guitare**, Piano, Autre). Génère deux pistes : `backing_track_no_guitar.wav` et `isolated_guitar.wav`.
2. **Détection de Tempo & Grille :** Extraction des BPM et des battements via `librosa` (pas madmom : ses modèles sont sous licence non commerciale).
3. **Transcription Audio-to-MIDI :** Passage de `isolated_guitar.wav` dans `Basic Pitch` (Spotify) pour extraire les notes avec début, durée et pitch bends.
4. **Solveur Biomécanique de Doigté (MIDI -> Tablature) :**
   - Sur une guitare, une même note existe sur plusieurs cordes/cases.
   - Implémenter l'algorithme de **Viterbi (programmation dynamique)** qui minimise une fonction de coût anatomique (distance entre deux positions de main, étirement des doigts, privilégier les cordes à vide et les positions anatomiques naturelles) pour transformer le MIDI en un fichier JSON `song_chart.json` (Corde + Frette + Timestamp).

## 4. Règles d'Ingénierie & Validation Automatisée
- **Zéro Mock en Production :** Chaque brique doit être fonctionnelle et vérifiée par un test automatisé avant de passer à l'étape suivante.
- **Tests DSP en CLI :** Le détecteur de pitch et le solveur de tablature doivent avoir des tests unitaires déterministes (ex: générer un signal sinusoïdal à 82.41 Hz -> vérifier que le moteur détecte instantanément la corde de Mi grave à vide `E2`).
- **Auto-documentation :** Maintenir l'état d'avancement dans `PROGRESS.md`.

## 5. Marques
Rocksmith est une marque d'Ubisoft. OpenSmith est un projet indépendant, sans lien avec Ubisoft : il ne contient ni contenu, ni code, ni clé de chiffrement du jeu. Le nom n'est cité que pour désigner le matériel compatible (Real Tone Cable).
