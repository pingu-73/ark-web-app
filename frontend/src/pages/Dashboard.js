import React, { useState, useEffect } from 'react';
import { 
  Container, 
  Header, 
  SpaceBetween, 
  Grid, 
  Box, 
  Button,
  StatusIndicator,
  Spinner,
  Tabs
} from '@cloudscape-design/components';
import { 
  getWalletInfo, 
  getWalletBalance, 
  getArkAddress, 
  getBoardingAddress, 
  getOnchainAddress,
  getAvailableBalance,
  getOnchainBalance,
  participateInRound 
} from '../api';

function Dashboard() {
  const [info, setInfo] = useState(null);
  const [balance, setBalance] = useState(null);
  const [arkAddress, setArkAddress] = useState('');
  const [boardingAddress, setBoardingAddress] = useState('');
  const [onchainAddress, setOnchainAddress] = useState('');
  const [availableBalance, setAvailableBalance] = useState(0);
  const [onchainBalance, setOnchainBalance] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [roundResult, setRoundResult] = useState(null);
  const [roundLoading, setRoundLoading] = useState(false);

  useEffect(() => {
    fetchData();
  }, []);

  const fetchData = async () => {
    try {
      setLoading(true);
      
      // Fetch all data in parallel
      const [
        infoData,
        balanceData,
        arkAddressData,
        boardingData,
        onchainAddressData,
        availableBalanceData,
        onchainBalanceData
      ] = await Promise.all([
        getWalletInfo(),
        getWalletBalance(),
        getArkAddress(),
        getBoardingAddress(),
        getOnchainAddress(),
        getAvailableBalance(),
        getOnchainBalance()
      ]);
      
      setInfo(infoData);
      setBalance(balanceData);
      setArkAddress(arkAddressData.address);
      setBoardingAddress(boardingData.address);
      setOnchainAddress(onchainAddressData.address);
      setAvailableBalance(availableBalanceData.available);
      setOnchainBalance(onchainBalanceData.balance);
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
      const result = await participateInRound();
      setRoundResult(result);
      // Refresh balances after round participation
      await fetchData();
    } catch (err) {
      setError('Failed to participate in round: ' + err.message);
    } finally {
      setRoundLoading(false);
    }
  };

  const copyToClipboard = (text) => {
    navigator.clipboard.writeText(text);
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
            { colspan: { default: 12, xxs: 4 } },
            { colspan: { default: 12, xxs: 4 } },
            { colspan: { default: 12, xxs: 4 } }
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

          {/* Ark Balance */}
          <Container
            header={
              <Header variant="h2">
                Ark Balance
              </Header>
            }
          >
            <SpaceBetween size="m">
              <div>
                <Box variant="awsui-key-label">Available</Box>
                <div>{availableBalance} sats</div>
              </div>
              <div>
                <Box variant="awsui-key-label">Confirmed</Box>
                <div>{balance?.confirmed} sats</div>
              </div>
              <div>
                <Box variant="awsui-key-label">Pending</Box>
                <div>{balance?.trusted_pending + balance?.untrusted_pending} sats</div>
              </div>
            </SpaceBetween>
          </Container>

          {/* On-Chain Balance */}
          <Container
            header={
              <Header variant="h2">
                On-Chain Balance
              </Header>
            }
          >
            <SpaceBetween size="m">
              <div>
                <Box variant="awsui-key-label">Available</Box>
                <div>{onchainBalance} sats</div>
              </div>
              {/* <div>
                <Box variant="awsui-key-label">Total</Box>
                <div>{onchainBalance} sats</div>
              </div> */}
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
          <Tabs
            tabs={[
              {
                label: "Ark Address",
                id: "ark",
                content: (
                  <SpaceBetween size="m">
                    <div>
                      <Header variant="h4">For receiving VTXOs (off-chain)</Header>
                      <Box variant="code">{arkAddress}</Box>
                      <Button
                        iconName="copy"
                        onClick={() => copyToClipboard(arkAddress)}
                      >
                        Copy
                      </Button>
                    </div>
                  </SpaceBetween>
                )
              },
              {
                label: "Boarding Address", 
                id: "boarding",
                content: (
                  <SpaceBetween size="m">
                    <div>
                      <Header variant="h4">For deposits into Ark (P2TR)</Header>
                      <Box variant="code">{boardingAddress}</Box>
                      <Button
                        iconName="copy"
                        onClick={() => copyToClipboard(boardingAddress)}
                      >
                        Copy
                      </Button>
                    </div>
                  </SpaceBetween>
                )
              },
              {
                label: "Bitcoin Address",
                id: "onchain", 
                content: (
                  <SpaceBetween size="m">
                    <div>
                      <Header variant="h4">For regular Bitcoin transactions (P2WPKH)</Header>
                      <Box variant="code">{onchainAddress}</Box>
                      <Button
                        iconName="copy"
                        onClick={() => copyToClipboard(onchainAddress)}
                      >
                        Copy
                      </Button>
                    </div>
                  </SpaceBetween>
                )
              }
            ]}
          />
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