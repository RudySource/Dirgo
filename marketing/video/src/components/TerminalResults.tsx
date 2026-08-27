import type {CSSProperties} from 'react';
import type {Candidate} from '../content/demo';
import {colors, radii, shadows} from '../design/tokens';
import {mono, typeScale} from '../design/typography';
import {KeyboardKey} from './KeyboardKey';

type TerminalResultsProps = {
  title: string;
  candidates: readonly Candidate[];
  selectedIndex: number;
  resultRange: string;
  action: 'open' | 'insert';
  actionActive?: boolean;
};

export const TerminalResults = ({
  title,
  candidates,
  selectedIndex,
  resultRange,
  action,
  actionActive = false,
}: TerminalResultsProps) => {
  const panelStyle: CSSProperties = {
    border: `1px solid ${colors.border}`,
    borderRadius: radii.panel,
    marginTop: 31,
    overflow: 'hidden',
  };

  return (
    <div style={panelStyle}>
      <div
        style={{
          alignItems: 'center',
          borderBottom: `1px solid ${colors.border}`,
          color: colors.textSecondary,
          display: 'flex',
          fontFamily: mono,
          fontSize: typeScale.utility,
          fontWeight: 650,
          letterSpacing: '0.08em',
          padding: '18px 23px',
          textTransform: 'uppercase',
        }}
      >
        <span>{title}</span>
        <span style={{color: colors.textQuiet, marginLeft: 'auto'}}>{resultRange}</span>
      </div>

      <div style={{padding: '12px 12px 10px'}}>
        {candidates.map((candidate, index) => {
          const selected = index === selectedIndex;
          return (
            <div
              key={`${candidate.label}-${candidate.detail}`}
              style={{
                alignItems: 'center',
                backgroundColor: selected ? colors.surfaceSelected : 'transparent',
                border: `1px solid ${selected ? 'rgba(32, 191, 85, 0.38)' : 'transparent'}`,
                borderRadius: radii.row,
                boxShadow: selected ? shadows.selection : 'none',
                display: 'grid',
                fontFamily: mono,
                gridTemplateColumns: candidate.source ? '42px 245px 84px 1fr' : '42px 260px 1fr',
                minHeight: candidates.length > 4 ? 55 : 64,
                padding: '0 18px',
              }}
            >
              <span
                style={{
                  color: selected ? colors.accent : 'transparent',
                  fontSize: 21,
                }}
              >
                ›
              </span>
              <span
                style={{
                  color: selected ? colors.text : '#C0C8CE',
                  fontSize: typeScale.terminalSmall,
                  fontWeight: selected ? 700 : 540,
                  letterSpacing: '-0.015em',
                }}
              >
                {candidate.label}
              </span>
              {candidate.source ? (
                <span
                  style={{
                    color: candidate.source === 'PROJ' ? colors.accentSoft : colors.textQuiet,
                    fontSize: 13,
                    fontWeight: 760,
                    letterSpacing: '0.12em',
                  }}
                >
                  {candidate.source}
                </span>
              ) : null}
              <span
                style={{
                  color: selected ? '#AAB5BD' : colors.textQuiet,
                  fontSize: 17,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                {candidate.detail}
              </span>
            </div>
          );
        })}
      </div>

      <div
        style={{
          alignItems: 'center',
          borderTop: `1px solid ${colors.border}`,
          color: colors.textQuiet,
          display: 'flex',
          fontFamily: mono,
          fontSize: 15,
          gap: 20,
          padding: '17px 23px 19px',
        }}
      >
        <span style={{display: 'flex', gap: 8}}>
          <KeyboardKey>↑</KeyboardKey>
          <KeyboardKey>↓</KeyboardKey>
        </span>
        <span>Select</span>
        <KeyboardKey active={actionActive}>{action === 'open' ? 'Enter' : 'Tab'}</KeyboardKey>
        <span>{action === 'open' ? 'Open' : 'Insert'}</span>
        <KeyboardKey>Esc</KeyboardKey>
        <span>Close</span>
      </div>
    </div>
  );
};
