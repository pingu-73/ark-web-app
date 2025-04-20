import React, { useState, useEffect } from 'react';
import { 
  Container, 
  Header, 
  SpaceBetween, 
  Grid, 
  Box, 
  Button,
  StatusIndicator,
  Spinner
} from '@cloudscape-design/components';

function Dashboard() {
  const [info, setInfo] = useState(null);
  const [balance, setBalance] = useState(null);
  const [address, setAddress] = useState('');
  const [boardingAddress, setBoardingAddress] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [roundResult, setRoundResult] = useState(null);
  const [roundLoading, setRoundLoading] = useState(false);

  const API_URL = process.env.REACT_APP_API_URL || 'http://localhost:3030/api';

  useEffect(() => {
    fetchData();
  }, []);

  const fetchData = async () => {
    try {
      setLoading(true);
      
      // Fetch wallet info
      const infoResponse = await fetch(`${API_URL}/wallet/info`);
      if (!infoResponse.ok) throw new Error(`Error fetching wallet info: ${infoResponse.statusText}`);
      const infoData = await infoResponse.json();
      
      // Fetch balance
      const balanceResponse = await fetch(`${API_URL}/wallet/balance`);
      if (!balanceResponse.ok) throw new Error(`Error fetching balance: ${balanceResponse.statusText}`);
      const balanceData = await balanceResponse.json();
      
      // Fetch address
      const addressResponse = await fetch(`${API_URL}/wallet/address`);
      if (!addressResponse.ok) throw new Error(`Error fetching address: ${addressResponse.statusText}`);
      const addressData = await addressResponse.json();
      
      // Fetch boarding address
      const boardingResponse = await fetch(`${API_URL}/wallet/boarding-address`);
      if (!boardingResponse.ok) throw new Error(`Error fetching boarding address: ${boardingResponse.statusText}`);
      const boardingData = await boardingResponse.json();
      
      setInfo(infoData);
      setBalance(balanceData);
      setAddress(addressData.address);
      setBoardingAddress(boardingData.address);
      setError(null);
    } catch (err) {
      setError('Failed to load wallet data: ' + err.message);
    } finally {
      setLoading(false);
    }
  };

  const handleParticipateInRound = async () => {
    try {
      setRoundLoading(true);
      const response = await fetch(`${API_URL}/round/participate`, {
        method: 'POST',
      });
      
      if (!response.ok) throw new Error(`Error participating in round: ${response.statusText}`);
      
      const result = await response.json();
      setRoundResult(result);
    } catch (err) {
      setError('Failed to participate in round: ' + err.message);
    } finally {
      setRoundLoading(false);
    }
  };

  if (loading) {
    return (
      <Container>
        <Spinner size="large" />
      </Container>
    );
  }

  return (
    <Container>
      <SpaceBetween size="l">
        <Header
          variant="h1"
          description="Manage your Bitcoin with Ark Protocol"
        >
          Dashboard
        </Header>

        {error && (
          <Box variant="error">
            {error}
          </Box>
        )}

        <Grid
          gridDefinition={[
            { colspan: { default: 12, xxs: 6 } },
            { colspan: { default: 12, xxs: 6 } }
          ]}
        >
          {/* Wallet Info */}
          <Container
            header={
              <Header variant="h2">
                Wallet Information
              </Header>
            }
          >
            <SpaceBetween size="m">
              <div>
                <Box variant="awsui-key-label">Network</Box>
                <div>{info?.network}</div>
              </div>
              <div>
                <Box variant="awsui-key-label">Server</Box>
                <div>{info?.server_url}</div>
              </div>
              <div>
                <Box variant="awsui-key-label">Status</Box>
                <StatusIndicator type={info?.connected ? "success" : "error"}>
                  {info?.connected ? "Connected" : "Disconnected"}
                </StatusIndicator>
              </div>
            </SpaceBetween>
          </Container>

          {/* Balance */}
          <Container
            header={
              <Header variant="h2">
                Balance
              </Header>
            }
          >
            <SpaceBetween size="m">
              <div>
                <Box variant="awsui-key-label">Confirmed</Box>
                <div>{balance?.confirmed} sats</div>
              </div>
              <div>
                <Box variant="awsui-key-label">Pending</Box>
                <div>{balance?.trusted_pending + balance?.untrusted_pending} sats</div>
              </div>
              <div>
                <Box variant="awsui-key-label">Total</Box>
                <div>{balance?.total} sats</div>
              </div>
            </SpaceBetween>
          </Container>
        </Grid>

        {/* Addresses */}
        <Container
          header={
            <Header variant="h2">
              Your Addresses
            </Header>
          }
        >
          <SpaceBetween size="l">
            <div>
              <Header variant="h3">Ark Address (for off-chain transactions)</Header>
              <Box variant="code">{address}</Box>
              <Button
                iconName="copy"
                onClick={() => {
                  navigator.clipboard.writeText(address);
                }}
              >
                Copy
              </Button>
            </div>
            
            <div>
              <Header variant="h3">Boarding Address (for on-chain deposits)</Header>
              <Box variant="code">{boardingAddress}</Box>
              <Button
                iconName="copy"
                onClick={() => {
                  navigator.clipboard.writeText(boardingAddress);
                }}
              >
                Copy
              </Button>
            </div>
          </SpaceBetween>
        </Container>

        {/* Round Participation */}
        <Container
          header={
            <Header variant="h2">
              Participate in Round
            </Header>
          }
        >
          <SpaceBetween size="m">
            <p>Participate in the next settlement round to batch your transactions.</p>
            <Button
              onClick={handleParticipateInRound}
              loading={roundLoading}
              variant="primary"
            >
              Participate Now
            </Button>
            
            {roundResult && (
              <Box>
                <p>Successfully participated in round!</p>
                <p>Transaction ID: {roundResult.txid || 'N/A'}</p>
              </Box>
            )}
          </SpaceBetween>
        </Container>
      </SpaceBetween>
    </Container>
  );
}

export default Dashboard;