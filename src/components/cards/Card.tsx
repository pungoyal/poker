import React from 'react';
import { Card as CardType, Suit, RANK_SYMBOLS, SUIT_SYMBOLS } from '../../types/card';
import './Card.css';

interface CardProps {
  card: CardType;
  faceDown?: boolean;
  small?: boolean;
}

const SUIT_CLASS: Record<Suit, string> = {
  [Suit.Hearts]: 'card-hearts',
  [Suit.Diamonds]: 'card-diamonds',
  [Suit.Clubs]: 'card-clubs',
  [Suit.Spades]: 'card-spades',
};

export const CardComponent: React.FC<CardProps> = ({ card, faceDown = false, small = false }) => {
  if (faceDown) {
    return (
      <div className={`card card-back ${small ? 'card-small' : ''}`}>
        <div className="card-back-pattern" />
      </div>
    );
  }

  const isRed = card.suit === Suit.Hearts || card.suit === Suit.Diamonds;
  const rankSymbol = RANK_SYMBOLS[card.rank];
  const suitSymbol = SUIT_SYMBOLS[card.suit];

  return (
    <div className={`card ${isRed ? 'card-red' : 'card-black'} ${SUIT_CLASS[card.suit]} ${small ? 'card-small' : ''}`}>
      <div className="card-corner card-corner-top">
        <span className="card-rank">{rankSymbol}</span>
        <span className="card-suit">{suitSymbol}</span>
      </div>
      <div className="card-center">
        <span className="card-suit-large">{suitSymbol}</span>
      </div>
      <div className="card-corner card-corner-bottom">
        <span className="card-rank">{rankSymbol}</span>
        <span className="card-suit">{suitSymbol}</span>
      </div>
    </div>
  );
};

export const CardBack: React.FC<{ small?: boolean }> = ({ small }) => (
  <div className={`card card-back ${small ? 'card-small' : ''}`}>
    <div className="card-back-pattern" />
  </div>
);
