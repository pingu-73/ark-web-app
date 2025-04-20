import React from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import SideNavigation from '@cloudscape-design/components/side-navigation';

function Navigation() {
  const navigate = useNavigate();
  const location = useLocation();
  
  const navItems = [
    { type: 'link', text: 'Dashboard', href: '/' },
    { type: 'link', text: 'Send', href: '/send' },
    { type: 'link', text: 'Transactions', href: '/transactions' },
    { type: 'link', text: 'Settings', href: '/settings' },
  ];

  const handleFollow = (e) => {
    e.preventDefault();
    navigate(e.detail.href);
  };

  return (
    <SideNavigation
      activeHref={location.pathname}
      header={{ text: 'Ark Web Wallet', href: '/' }}
      items={navItems}
      onFollow={handleFollow}
    />
  );
}

export default Navigation;