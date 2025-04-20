import React, { useState, useEffect } from 'react';
import {
  Container,
  Header,
  SpaceBetween,
  FormField,
  Input,
  Select,
  Button,
  Box,
  Alert,
  SegmentedControl,
  StatusIndicator
} from '@cloudscape-design/components';
import { getWalletInfo } from '../api';

function Settings() {
  const [info, setInfo] = useState(null);
  const [isLoading, setIsLoading] = useState(true);  // Renamed to avoid unused variable
  const [error, setError] = useState(null);
  const [theme, setTheme] = useState('dark');

  useEffect(() => {
    const fetchData = async () => {
      try {
        setIsLoading(true);
        const infoData = await getWalletInfo();
        setInfo(infoData);
        setError(null);
      } catch (err) {
        setError('Failed to load wallet info: ' + err.message);
      } finally {
        setIsLoading(false);
      }
    };

    fetchData();
  }, []);

  const handleThemeChange = ({ detail }) => {
    setTheme(detail.selectedId);
    // In a real app, you would apply the theme here
  };

  return (
    <Container>
      <SpaceBetween size="l">
        <Header
          variant="h1"
          description="Configure your wallet settings"
        >
          Settings
        </Header>

        {error && (
          <Alert type="error" header="Error">
            {error}
          </Alert>
        )}

        <Container
          header={
            <Header variant="h2">
              Appearance
            </Header>
          }
        >
          <SpaceBetween size="m">
            <FormField
              label="Theme"
              description="Choose between light and dark theme"
            >
              <SegmentedControl
                selectedId={theme}
                onChange={handleThemeChange}
                options={[
                  { id: 'light', text: 'Light' },
                  { id: 'dark', text: 'Dark' }
                ]}
              />
            </FormField>
          </SpaceBetween>
        </Container>

        <Container
          header={
            <Header variant="h2">
              Network
            </Header>
          }
        >
          <SpaceBetween size="m">
            <FormField
              label="Current Network"
              description="The Bitcoin network you're connected to"
            >
              <Input
                value={info?.network || ''}
                disabled
              />
            </FormField>

            <FormField
              label="Ark Server URL"
              description="The URL of the Ark server"
            >
              <Input
                value={info?.server_url || ''}
                disabled
              />
            </FormField>

            <Box>
              <StatusIndicator type={info?.connected ? "success" : "error"}>
                {info?.connected ? "Connected to Ark server" : "Disconnected from Ark server"}
              </StatusIndicator>
            </Box>
          </SpaceBetween>
        </Container>

        <Container
          header={
            <Header variant="h2">
              Advanced
            </Header>
          }
        >
          <SpaceBetween size="m">
            <FormField
              label="Fee Rate (sats/vB)"
              description="Default fee rate for transactions"
            >
              <Select
                selectedOption={{ label: "1 sat/vB (Economy)", value: "1" }}
                options={[
                  { label: "1 sat/vB (Economy)", value: "1" },
                  { label: "2 sat/vB (Standard)", value: "2" },
                  { label: "5 sat/vB (Priority)", value: "5" }
                ]}
              />
            </FormField>

            <Button variant="primary">
              Save Settings
            </Button>
          </SpaceBetween>
        </Container>
      </SpaceBetween>
    </Container>
  );
}

export default Settings;