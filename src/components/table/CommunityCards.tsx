import React from 'react';
import { Card } from '../../types/card';
import { CardComponent } from '../cards/Card';
import './CommunityCards.css';

interface CommunityCardsProps {
  cards: Card[];
}

export const CommunityCards: React.FC<CommunityCardsProps> = ({ cards }) => {
  if (cards.length === 0) return null;

  return (
    <div className="community-cards">
      {cards.map((card, idx) => (
        <div key={idx} className="community-card-wrapper" style={{ animationDelay: `${idx * 70}ms` }}>
          <CardComponent card={card} />
        </div>
      ))}
    </div>
  );
};
