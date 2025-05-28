import React, { useState, useEffect } from 'react';
import {
  Container,
  Header,
  SpaceBetween,
  Button,
  Box,
  ColumnLayout,
  Tabs,
  Alert
} from '@cloudscape-design/components';
import { 
  getArkAddress, 
  getBoardingAddress, 
  getOnchainAddress 
} from '../api';

function Receive() {
  const [arkAddress, setArkAddress] = useState('');
  const [boardingAddress, setBoardingAddress] = useState('');
  const [onchainAddress, setOnchainAddress] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  useEffect(() => {
    fetchAddresses();
  }, []);

  const fetchAddresses = async () => {
    try {
      setLoading(true);
      
      const [arkData, boardingData, onchainData] = await Promise.all([
        getArkAddress(),
        getBoardingAddress(),
        getOnchainAddress()
      ]);
      
      setArkAddress(arkData.address);
      setBoardingAddress(boardingData.address);
      setOnchainAddress(onchainData.address);
      setError(null);
    } catch (err) {
      setError('Failed to load addresses: ' + err.message);
    } finally {
      setLoading(false);
    }
  };

  const copyToClipboard = (text, type) => {
    navigator.clipboard.writeText(text);
    // You could add a toast notification here
  };

  const generateQRCode = (address) => {
    // You could integrate a QR code library here
    return `https://api.qrserver.com/v1/create-qr-code/?size=200x200&data=${address}`;
  };

  if (loading) {
    return (
      <Container>
        <Header variant="h1">Loading addresses...</Header>
      </Container>
    );
  }

  return (
    <Container>
      <SpaceBetween size="l">
        <Header
          variant="h1"
          description="Share these addresses to receive Bitcoin"
        >
          Receive Bitcoin
        </Header>
        
        {error && (
          <Alert type="error" header="Error">
            {error}
          </Alert>
        )}
        
        <Tabs
          tabs={[
            {
              label: "Ark Address (VTXOs)",
              id: "ark",
              content: (
                <Container>
                  <SpaceBetween size="l">
                    <Alert type="info">
                      Use this address to receive instant, low-fee VTXO payments from other Ark users.
                    </Alert>
                    
                    <ColumnLayout columns={2}>
                      <div>
                        <Header variant="h3">Your Ark Address</Header>
                        <Box variant="code" fontSize="body-s">
                          {arkAddress}
                        </Box>
                        <SpaceBetween size="s" direction="horizontal">
                          <Button
                            iconName="copy"
                            onClick={() => copyToClipboard(arkAddress, 'Ark')}
                          >
                            Copy Address
                          </Button>
                        </SpaceBetween>
                      </div>
                      
                      <div>
                        <Header variant="h3">QR Code</Header>
                        <img 
                          src={generateQRCode(arkAddress)} 
                          alt="Ark Address QR Code"
                          style={{ maxWidth: '200px' }}
                        />
                      </div>
                    </ColumnLayout>
                  </SpaceBetween>
                </Container>
              )
            },
            {
              label: "Boarding Address (Deposits)",
              id: "boarding",
              content: (
                <Container>
                  <SpaceBetween size="l">
                    <Alert type="warning">
                      Use this address to deposit Bitcoin into the Ark system. Funds sent here will be available for Ark operations after participating in a round.
                    </Alert>
                    
                    <ColumnLayout columns={2}>
                      <div>
                        <Header variant="h3">Your Boarding Address</Header>
                        <Box variant="code" fontSize="body-s">
                          {boardingAddress}
                        </Box>
                        <SpaceBetween size="s" direction="horizontal">
                          <Button
                            iconName="copy"
                            onClick={() => copyToClipboard(boardingAddress, 'Boarding')}
                          >
                            Copy Address
                          </Button>
                        </SpaceBetween>
                      </div>
                      
                      <div>
                        <Header variant="h3">QR Code</Header>
                        <img 
                          src={generateQRCode(boardingAddress)} 
                          alt="Boarding Address QR Code"
                          style={{ maxWidth: '200px' }}
                        />
                      </div>
                    </ColumnLayout>
                  </SpaceBetween>
                </Container>
              )
            },
            {
              label: "Bitcoin Address (On-chain)",
              id: "onchain",
              content: (
                <Container>
                  <SpaceBetween size="l">
                    <Alert type="info">
                      Use this address to receive regular Bitcoin transactions. These funds can be used for on-chain payments.
                    </Alert>
                    
                    <ColumnLayout columns={2}>
                      <div>
                        <Header variant="h3">Your Bitcoin Address</Header>
                        <Box variant="code" fontSize="body-s">
                          {onchainAddress}
                        </Box>
                        <SpaceBetween size="s" direction="horizontal">
                          <Button
                            iconName="copy"
                            onClick={() => copyToClipboard(onchainAddress, 'Bitcoin')}
                          >
                            Copy Address
                          </Button>
                        </SpaceBetween>
                      </div>
                      
                      <div>
                        <Header variant="h3">QR Code</Header>
                        <img 
                          src={generateQRCode(onchainAddress)} 
                          alt="Bitcoin Address QR Code"
                          style={{ maxWidth: '200px' }}
                        />
                      </div>
                    </ColumnLayout>
                  </SpaceBetween>
                </Container>
              )
            }
          ]}
        />
        
        <Container
          header={
            <Header variant="h2">
              Address Usage Guide
            </Header>
          }
        >
          <SpaceBetween size="m">
            <div>
              <Header variant="h4">🚀 Ark Address (VTXOs)</Header>
              <p>For receiving instant, low-fee payments from other Ark users. These transactions happen off-chain and settle instantly.</p>
            </div>
            
            <div>
              <Header variant="h4">🏦 Boarding Address (Deposits)</Header>
              <p>For depositing Bitcoin into the Ark system. Send regular Bitcoin here to enter the Ark protocol. Remember to participate in a round after depositing.</p>
            </div>
            
            <div>
              <Header variant="h4">₿ Bitcoin Address (On-chain)</Header>
              <p>For receiving regular Bitcoin transactions. These funds can be used for standard on-chain payments to any Bitcoin address.</p>
            </div>
          </SpaceBetween>
        </Container>
      </SpaceBetween>
    </Container>
  );
}

export default Receive;