import React, { useState, useEffect } from 'react';
import {
  Container,
  Header,
  Table,
  Box,
  Spinner,
  StatusIndicator,
  TextFilter,
  Pagination,
  CollectionPreferences,
  SpaceBetween
} from '@cloudscape-design/components';

function Transactions() {
  const [transactions, setTransactions] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [filterText, setFilterText] = useState('');
  const [currentPage, setCurrentPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);

  const API_URL = process.env.REACT_APP_API_URL || 'http://localhost:3030/api';

  useEffect(() => {
    fetchTransactions();
  }, []);

  const fetchTransactions = async () => {
    try {
      setLoading(true);
      const response = await fetch(`${API_URL}/transactions`);
      
      if (!response.ok) {
        throw new Error(`Error fetching transactions: ${response.statusText}`);
      }
      
      const data = await response.json();
      setTransactions(data);
      setError(null);
    } catch (err) {
      setError('Failed to load transactions: ' + err.message);
    } finally {
      setLoading(false);
    }
  };

  const formatDate = (timestamp) => {
    return new Date(timestamp * 1000).toLocaleString();
  };

  // Filter transactions based on search text
  const filteredTransactions = transactions.filter(tx => 
    tx.txid.includes(filterText) || 
    tx.type_name.toLowerCase().includes(filterText.toLowerCase())
  );

  // Paginate transactions
  const paginatedTransactions = filteredTransactions.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize
  );

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
          description="View your transaction history"
        >
          Transactions
        </Header>

        {error && (
          <Box variant="error">
            {error}
          </Box>
        )}

        <Table
          header={
            <Header
              counter={`(${filteredTransactions.length})`}
              actions={
                <TextFilter
                  filteringText={filterText}
                  onChange={({ detail }) => setFilterText(detail.filteringText)}
                  placeholder="Find transactions"
                />
              }
            >
              Transactions
            </Header>
          }
          columnDefinitions={[
            {
              id: 'type',
              header: 'Type',
              cell: item => item.type_name,
              sortingField: 'type_name'
            },
            {
              id: 'amount',
              header: 'Amount (sats)',
              cell: item => (
                <span style={{ color: item.amount > 0 ? 'green' : 'red' }}>
                  {item.amount > 0 ? '+' : ''}{item.amount}
                </span>
              ),
              sortingField: 'amount'
            },
            {
              id: 'date',
              header: 'Date',
              cell: item => formatDate(item.timestamp),
              sortingField: 'timestamp'
            },
            {
              id: 'status',
              header: 'Status',
              cell: item => (
                item.is_settled !== undefined ? (
                  <StatusIndicator type={item.is_settled ? "success" : "pending"}>
                    {item.is_settled ? 'Settled' : 'Pending'}
                  </StatusIndicator>
                ) : 'N/A'
              )
            },
            {
              id: 'txid',
              header: 'Transaction ID',
              cell: item => (
                <span title={item.txid}>
                  {item.txid.substring(0, 10)}...
                </span>
              )
            }
          ]}
          items={paginatedTransactions}
          pagination={
            <Pagination
              currentPageIndex={currentPage}
              pagesCount={Math.ceil(filteredTransactions.length / pageSize)}
              onChange={({ detail }) => setCurrentPage(detail.currentPageIndex)}
            />
          }
          preferences={
            <CollectionPreferences
              title="Preferences"
              confirmLabel="Confirm"
              cancelLabel="Cancel"
              preferences={{
                pageSize
              }}
              pageSizePreference={{
                title: "Page size",
                options: [
                  { value: 10, label: "10 transactions" },
                  { value: 20, label: "20 transactions" },
                  { value: 50, label: "50 transactions" }
                ]
              }}
              onConfirm={({ detail }) => setPageSize(detail.pageSize)}
            />
          }
          empty={
            <Box textAlign="center" color="inherit">
              <b>No transactions</b>
              <Box padding={{ bottom: "s" }} variant="p" color="inherit">
                No transactions to display.
              </Box>
            </Box>
          }
        />
      </SpaceBetween>
    </Container>
  );
}

export default Transactions;