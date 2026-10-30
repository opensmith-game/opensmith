import { describe, expect, it } from 'vitest';
import { formatDuration, formatSize, jobStatus, pipelineSteps, queueSummary, shortPath, visibleJobs, type Job } from '../src/imports';

const job = (over: Partial<Job>): Job => ({
  id: 1,
  source: '/Users/me/Music/Night Drive.mp3',
  name: 'Night Drive.mp3',
  state: 'queued',
  stage: null,
  fraction: 0,
  elapsed: 0,
  song: null,
  songs: 0,
  title: null,
  error: null,
  target: null,
  ...over,
});

describe('song imports', () => {
  it('shows an MP3 going through separation, tempo and notes, fingering, into the library', () => {
    const states = (j: Job) => pipelineSteps(j).map((s) => s.state);
    expect(pipelineSteps(job({})).map((s) => s.label)).toEqual(['Séparation des pistes', 'Tempo et notes', 'Doigté du manche', 'Bibliothèque']);
    expect(states(job({}))).toEqual(['pending', 'pending', 'pending', 'pending']);
    const separating = pipelineSteps(job({ state: 'running', stage: 'separate', fraction: 0.3 }));
    expect(separating.map((s) => s.state)).toEqual(['active', 'pending', 'pending', 'pending']);
    expect(separating[0].progress).toBeCloseTo(0.3 / 0.59);
    expect(states(job({ state: 'running', stage: 'beats', fraction: 0.8 }))).toEqual(['done', 'active', 'pending', 'pending']);
    expect(states(job({ state: 'running', stage: 'chart', fraction: 0.95 }))).toEqual(['done', 'done', 'active', 'pending']);
    expect(states(job({ state: 'done', fraction: 1 }))).toEqual(['done', 'done', 'done', 'done']);
    expect(states(job({ state: 'failed', stage: 'transcribe', fraction: 0.7 }))).toEqual(['done', 'failed', 'pending', 'pending']);
  });

  it('shows a tab, or a file a plugin imports, read in one step', () => {
    const other = pipelineSteps(job({ source: '/Music/songs.arc', state: 'running', fraction: 0.4 }));
    expect(other.map((s) => [s.label, s.state])).toEqual([
      ['Lecture du fichier', 'active'],
      ['Bibliothèque', 'pending'],
    ]);
    expect(pipelineSteps(job({ source: '/Music/night-drive.gp5' }))[0].label).toBe('Lecture de la tablature');
  });

  it('shows the converter being installed before the first import of a built app', () => {
    const installing = pipelineSteps(job({ state: 'running', stage: 'install', fraction: 0.5, detail: 'Downloaded torch' }));
    expect(installing.map((s) => [s.label, s.state])).toEqual([
      ['Installation du convertisseur', 'active'],
      ['Séparation des pistes', 'pending'],
      ['Tempo et notes', 'pending'],
      ['Doigté du manche', 'pending'],
      ['Bibliothèque', 'pending'],
    ]);
    expect(installing[0]).toMatchObject({ progress: 0.5, tech: 'une seule fois · environ 2 Go · Downloaded torch' });
    expect(jobStatus(job({ state: 'running', stage: 'install', fraction: 0.25 }))).toBe('Installation du convertisseur · 25 %');
    const failed = pipelineSteps(job({ state: 'failed', stage: 'install', fraction: 0.3 }));
    expect(failed.map((s) => s.state)).toEqual(['failed', 'pending', 'pending', 'pending', 'pending']);
  });

  it('says where each import stands', () => {
    expect(jobStatus(job({}))).toBe('En attente');
    expect(jobStatus(job({ state: 'running', stage: 'separate', fraction: 0.427 }))).toBe('Séparation de la guitare · 42 %');
    expect(jobStatus(job({ state: 'running' }))).toBe('Démarrage · 0 %');
    expect(jobStatus(job({ state: 'done', elapsed: 34.4 }))).toBe('Prêt en 34 s');
    expect(jobStatus(job({ state: 'failed', error: 'ffmpeg could not read x.mp3' }))).toBe('Échec : ffmpeg could not read x.mp3');
    expect(jobStatus(job({ state: 'cancelled' }))).toBe('Annulé');
  });

  it('formats durations, sizes and Mac paths for people', () => {
    expect(formatDuration(75)).toBe('1 min 15');
    expect(formatSize(8_460_000)).toBe('8,5 Mo');
    expect(shortPath('/Users/me/Music/A jouer')).toBe('~/Music/A jouer');
    expect(shortPath('/Volumes/Disque/Musique')).toBe('/Volumes/Disque/Musique');
  });

  it('sums up a folder of archives going through the queue', () => {
    const archive = (id: number, over: Partial<Job> = {}) => job({ id, source: `/Music/album/song${id}.mp3`, name: `song${id}.mp3`, ...over });
    const jobs = [
      archive(1, { state: 'done', fraction: 1 }),
      archive(2, { state: 'done', fraction: 1 }),
      archive(3, { state: 'failed', error: 'archive illisible' }),
      archive(4, { state: 'running', fraction: 0.5 }),
      ...Array.from({ length: 250 }, (_, i) => archive(5 + i)),
    ];
    const s = queueSummary(jobs, false);
    expect(s).toMatchObject({ total: 254, finished: 3, done: 2, failed: 1, waiting: 250, paused: false });
    expect(s.running?.id).toBe(4);
    expect(s.fraction).toBeCloseTo(3.5 / 254);
    expect(queueSummary([], true).fraction).toBe(0);

    // Hundreds of imports: rows for the one under way, the failures and the latest songs only.
    const { rows, waiting } = visibleJobs([...jobs, ...Array.from({ length: 9 }, (_, i) => archive(300 + i, { state: 'done', fraction: 1 }))]);
    expect(rows.map((j) => j.id)).toEqual([4, 3, 308, 307, 306, 305, 304]);
    expect(waiting).toHaveLength(250);
    expect(waiting[0].id).toBe(5);
  });
});
