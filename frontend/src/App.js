import React from 'react';
import { BrowserRouter, Routes, Route, Link } from 'react-router-dom';
import { Container, Header, SpaceBetween } from '@cloudscape-design/components';

// Import pages
import Dashboard from './pages/Dashboard';
import Send from './pages/Send';
import Transactions from './pages/Transactions';

function App() {
  return (
    <BrowserRouter>
      <Container>
        <SpaceBetween size="m">
          <Header variant="h1">Ark Web Wallet</Header>
          
          <div style={{ display: 'flex', gap: '20px', marginBottom: '20px' }}>
            <Link to="/">Dashboard</Link>
            <Link to="/send">Send</Link>
            <Link to="/transactions">Transactions</Link>
          </div>
          
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/send" element={<Send />} />
            <Route path="/transactions" element={<Transactions />} />
          </Routes>
        </SpaceBetween>
      </Container>
    </BrowserRouter>
  );
}

export default App;