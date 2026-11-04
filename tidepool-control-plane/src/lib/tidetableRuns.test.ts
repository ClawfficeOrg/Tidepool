import { describe, expect, it } from 'vitest';
import {
  buildRunRequest,
  jobIsRunning,
  jobStatusLine,
  parseTaskIds,
  type TidetableRunInput,
} from './tidetableRuns';

const valid: TidetableRunInput = {
  projectId: '9ee504eb-d74d-11f1-acdb-629afb145d13',
  host: '322314d7-a610-44a5-bf29-8d1c4a1963d8',
  checkout: 'oarlock',
  tasks: '1.22.6 1.22.7',
  hours: '2',
  jobs: '1',
  dryRun: true,
};

describe('parseTaskIds', () => {
  it('splits on spaces and commas and keeps the order', () => {
    expect(parseTaskIds('1.22.6, 1.22.7  1.3.1')).toEqual({
      ids: ['1.22.6', '1.22.7', '1.3.1'],
      problems: [],
    });
  });

  it('refuses an empty list, a bad id, and too many ids', () => {
    expect(parseTaskIds('   ').problems).toEqual(['Name at least one task id, such as 1.22.6.']);
    expect(parseTaskIds('1.22').problems[0]).toContain('"1.22" is not a task id');
    const many = Array.from({ length: 17 }, (_, index) => `1.1.${index}`).join(' ');
    expect(parseTaskIds(many).problems[0]).toContain('at most 16');
  });
});

describe('buildRunRequest', () => {
  it('builds the body the pool takes, with dry run as typed', () => {
    const built = buildRunRequest(valid);
    expect(built.problems).toEqual([]);
    expect(built.request).toEqual({
      host: valid.host,
      checkout: 'oarlock',
      options: { hours: 2, jobs: 1, tasks: ['1.22.6', '1.22.7'], dryRun: true },
    });
  });

  it('sends nothing until every field is valid, and says which ones are not', () => {
    const built = buildRunRequest({ ...valid, host: ' ', checkout: 'Oarlock', hours: '13', jobs: '5' });
    expect(built.request).toBeNull();
    expect(built.problems).toEqual([
      'Choose the host that should run the job.',
      'The checkout name is lowercase letters, digits, dashes or underscores, as the host names it.',
      'Hours is a whole number from 1 to 12.',
      'Parallel jobs is a whole number from 1 to 4.',
    ]);
  });

  it('refuses a fractional or signed number rather than rounding it', () => {
    expect(buildRunRequest({ ...valid, hours: '1.5' }).problems).toContain(
      'Hours is a whole number from 1 to 12.',
    );
    expect(buildRunRequest({ ...valid, jobs: '-1' }).problems).toContain(
      'Parallel jobs is a whole number from 1 to 4.',
    );
  });
});

describe('job status', () => {
  it('treats queued and running as in progress, everything else as finished', () => {
    expect(jobIsRunning('queued')).toBe(true);
    expect(jobIsRunning('running')).toBe(true);
    expect(jobIsRunning('succeeded')).toBe(false);
    expect(jobIsRunning('refused')).toBe(false);
  });

  it('names a refusal in plain words', () => {
    expect(jobStatusLine({ status: 'refused', refusalCode: 'tkrun_missing' })).toBe(
      'Refused (tkrun_missing). The host did not start the run.',
    );
    expect(jobStatusLine({ status: 'running', refusalCode: null })).toBe('Running on the host.');
  });
});
